use std::io::Write;
use std::process::{Command, Stdio};

use crypto::keys::Key;
use crypto::PrivateKey;
use db::block::builder::ClientCreateSignatures;
use defra_core::block::{
    Block, CrdtDelta, Signature, SignatureHeader, SignatureType, IMMUTABLE_LINK_NAME,
};
use defra_core::signing::{SigningConfig, SigningKeyType};
use document::{Document, NormalValue};
use schema::{CType, CollectionVersion, FieldDescription, FieldKind};
use serde_json::{json, Value};

fn fixture() -> (Document, SigningConfig) {
    let schema = CollectionVersion::new(
        "Agent",
        "trusted-agent-version",
        "trusted-agent",
        vec![
            FieldDescription::new("1", "agent_did", FieldKind::string(), CType::LwwRegister)
                .as_immutable(),
            FieldDescription::new(
                "2",
                "creation_nonce",
                FieldKind::string(),
                CType::LwwRegister,
            )
            .as_immutable(),
            FieldDescription::new("3", "node_did", FieldKind::string(), CType::LwwRegister)
                .as_immutable(),
            FieldDescription::new("4", "display_name", FieldKind::string(), CType::LwwRegister),
            FieldDescription::new(
                "5",
                "tags",
                FieldKind::nillable_string_array(),
                CType::LwwRegister,
            ),
        ],
    );
    let mut doc = Document::new(schema);
    doc.set("agent_did", NormalValue::String("did:key:agent".into()))
        .unwrap();
    doc.set("creation_nonce", NormalValue::String("nonce".into()))
        .unwrap();
    doc.set("node_did", NormalValue::String("did:key:node".into()))
        .unwrap();
    doc.set("display_name", NormalValue::String("Example".into()))
        .unwrap();
    doc.set(
        "tags",
        NormalValue::NillableStringArray(Some(vec!["one".into(), "two".into()])),
    )
    .unwrap();
    let private = crypto::generate_ed25519().unwrap();
    let public = private.public_key();
    let public_key_bytes = public.raw_owned();
    let signer = SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: private.raw_owned(),
        public_key_hex: hex::encode(&public_key_bytes),
        public_key_bytes,
        remote_signer: None,
        signing_authorization: None,
    };
    (doc, signer)
}

fn model(file: &str, input: &Value) -> Value {
    let mut child = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", file])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{input}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn intended_fields(doc: &Document) -> Vec<Value> {
    let mut result = doc
        .values()
        .iter()
        .map(|(name, value)| {
            json!({
                "name": name,
                "value": document::encoding::normal_value_to_json(value.value()).unwrap(),
                "immutable": doc.collection().field_by_name(name).unwrap().immutable,
            })
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    result
}

fn decoded_field(name: &str, data: &[u8], immutable: bool) -> Value {
    let cbor = ciborium::from_reader(data).unwrap();
    let value = document::encoding::cbor_to_normal_value(cbor).unwrap();
    json!({"name": name, "value": document::encoding::normal_value_to_json(&value).unwrap(), "immutable": immutable})
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn normalized_agent_intent_preserves_explicit_null_and_datetime() {
    use query::plan::mutation::AddInput;

    let (_, signer) = fixture();
    let schema = CollectionVersion::new(
        "Agent",
        "trusted-agent-version",
        "trusted-agent",
        vec![
            FieldDescription::new("1", "agent_did", FieldKind::string(), CType::LwwRegister)
                .as_immutable(),
            FieldDescription::new(
                "2",
                "creation_nonce",
                FieldKind::string(),
                CType::LwwRegister,
            )
            .as_immutable(),
            FieldDescription::new("3", "node_did", FieldKind::string(), CType::LwwRegister)
                .as_immutable(),
            FieldDescription::new("4", "description", FieldKind::string(), CType::LwwRegister),
            FieldDescription::new(
                "5",
                "tags",
                FieldKind::nillable_string_array(),
                CType::LwwRegister,
            ),
            FieldDescription::new("6", "updated_at", FieldKind::datetime(), CType::LwwRegister),
            FieldDescription::new(
                "7",
                "times",
                FieldKind::datetime_array(),
                CType::LwwRegister,
            ),
        ],
    );
    let approved = json!({"agent_did":"did:key:agent", "creation_nonce":"nonce", "node_did":"did:key:node", "description":null, "tags":["one","two"], "updated_at":"2026-08-14T12:00:00.120000000Z"});
    let document = |value: &Value| {
        AddInput {
            fields: value
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }
        .to_document_with_schema(&schema)
        .unwrap()
    };
    let fields = |doc: &Document| {
        let mut fields = doc
            .to_map()
            .unwrap()
            .into_iter()
            .map(|(name, value)| json!({"name":name,"value":value.to_string()}))
            .collect::<Vec<_>>();
        fields.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        fields
    };
    let mut trimmed = approved.clone();
    trimmed["updated_at"] = json!("2026-08-14T12:00:00.12Z");
    let mut omitted = approved.clone();
    omitted.as_object_mut().unwrap().remove("description");
    let mut changed = approved.clone();
    changed["description"] = json!("changed");
    let mut tags = approved.clone();
    tags["tags"] = json!(["two", "one"]);
    let mut time = approved.clone();
    time["updated_at"] = json!("2026-08-14T12:00:00.13Z");
    let cases = [approved.clone(), trimmed, omitted, changed, tags, time];
    let doc = document(&approved);
    let graph = ClientCreateSignatures::new(&storage::Crdts::default(), &doc, &signer).unwrap();
    let from_input =
        ClientCreateSignatures::from_input(&storage::Crdts::default(), &schema, &approved, &signer)
            .unwrap();
    assert_eq!(from_input.doc_id(), graph.doc_id());
    let mut from_input_blocks = from_input.child_blocks().to_vec();
    let mut graph_blocks = graph.child_blocks().to_vec();
    from_input_blocks.sort();
    graph_blocks.sort();
    assert_eq!(from_input_blocks, graph_blocks);
    let model_inputs = cases
        .iter()
        .map(|input| json!({"approved":fields(&doc), "offered":fields(&document(input)), "generated":false}))
        .collect::<Vec<_>>();
    let expected = model("ClientCreateIntent.lean", &json!(model_inputs));
    assert_eq!(
        expected
            .as_array()
            .unwrap()
            .iter()
            .map(|case| case["accepted"].as_bool().unwrap())
            .collect::<Vec<_>>(),
        vec![true, true, false, false, false, false]
    );
    for (index, input) in cases.iter().enumerate() {
        assert_eq!(
            graph.matches_input("Agent", input),
            expected[index]["accepted"].as_bool().unwrap()
        );
    }
    let mut null_list = approved.clone();
    null_list["tags"] = Value::Null;
    let mut omitted_list = null_list.clone();
    omitted_list.as_object_mut().unwrap().remove("tags");
    let list_doc = document(&null_list);
    let list_graph = ClientCreateSignatures::from_input(
        &storage::Crdts::default(),
        &schema,
        &null_list,
        &signer,
    )
    .unwrap();
    let list_model = model(
        "ClientCreateIntent.lean",
        &json!([
            {"approved":fields(&list_doc), "offered":fields(&document(&null_list)), "generated":false},
            {"approved":fields(&list_doc), "offered":fields(&document(&omitted_list)), "generated":false}
        ]),
    );
    assert!(list_model[0]["accepted"].as_bool().unwrap());
    assert!(!list_model[1]["accepted"].as_bool().unwrap());
    assert_eq!(
        list_graph.matches_input("Agent", &null_list),
        list_model[0]["accepted"].as_bool().unwrap()
    );
    assert_eq!(
        list_graph.matches_input("Agent", &omitted_list),
        list_model[1]["accepted"].as_bool().unwrap()
    );
    let generated = model(
        "ClientCreateIntent.lean",
        &json!([
            {"approved":fields(&doc), "offered":fields(&doc), "generated":true}
        ]),
    );
    assert!(!generated[0]["accepted"].as_bool().unwrap());
    let mut scalar_now = approved.clone();
    scalar_now["updated_at"] = json!("UTC_NOW");
    let mut array_now = approved.clone();
    array_now["times"] = json!(["2026-08-14T12:00:00Z", "UTC_NOW"]);
    for input in [&scalar_now, &array_now] {
        assert_eq!(
            graph.matches_input("Agent", input),
            generated[0]["accepted"].as_bool().unwrap()
        );
        assert!(ClientCreateSignatures::from_input(
            &storage::Crdts::default(),
            &schema,
            input,
            &signer
        )
        .is_err());
    }
    let mut literal = approved.clone();
    literal["description"] = json!("UTC_NOW");
    assert!(ClientCreateSignatures::from_input(
        &storage::Crdts::default(),
        &schema,
        &literal,
        &signer
    )
    .is_ok());
    let null_block = graph.child_blocks().iter().map(|bytes| Block::from_dag_cbor(bytes).unwrap()).find(|block| matches!(&block.delta, CrdtDelta::Lww(delta) if delta.field_name == "description")).unwrap();
    let CrdtDelta::Lww(null) = null_block.delta else {
        unreachable!()
    };
    assert_eq!(
        decoded_field("description", &null.data, false)["value"],
        Value::Null
    );
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn trusted_create_graph_matches_model_and_rejects_unapproved_waves() {
    let (doc, signer) = fixture();
    let expected_fields = intended_fields(&doc);
    let mut prepared =
        ClientCreateSignatures::new(&storage::Crdts::default(), &doc, &signer).unwrap();
    let children = prepared.child_blocks().to_vec();
    let mut immutable = Vec::new();
    let mut mutable = Vec::new();
    for bytes in &children {
        let block = Block::from_dag_cbor(bytes).unwrap();
        match block.delta {
            CrdtDelta::Immutable(value) => {
                for (name, data) in value.fields {
                    immutable.push(decoded_field(&name, &data, true));
                }
            }
            CrdtDelta::Lww(value) => {
                mutable.push(decoded_field(&value.field_name, &value.data, false))
            }
            other => panic!("unexpected Agent field delta {other:?}"),
        }
    }
    immutable.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    mutable.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let graph = model(
        "ClientCreateGraph.lean",
        &json!([{
            "fields": expected_fields, "expectedLinks": [], "offeredLinks": []
        }]),
    );
    assert_eq!(json!(immutable), graph[0]["immutableFields"]);
    assert_eq!(json!(mutable), graph[0]["mutableFields"]);

    let expected = children.iter().map(hex::encode).collect::<Vec<_>>();
    let mut altered = children.clone();
    altered[0][0] ^= 1;
    let mut duplicate = children.clone();
    duplicate[0] = duplicate[1].clone();
    let mut reordered = children.clone();
    reordered.reverse();
    let cases = [
        children.clone(),
        reordered,
        altered,
        children[..children.len() - 1].to_vec(),
        duplicate,
    ];
    let model_inputs = cases.iter().map(|wave| json!({
        "capsule": {"actor":"alice", "transaction":"tx", "operation":"op", "session":"s", "wave":0, "offered":expected, "live":true},
        "intent": {"expected":expected},
        "reply": {"actor":"alice", "transaction":"tx", "operation":"op", "session":"s", "wave":0, "blocks":wave.iter().map(hex::encode).collect::<Vec<_>>(), "keyMatchesActor":true, "signaturesValid":true}
    })).collect::<Vec<_>>();
    let expectations = model("ClientSignedCreate.lean", &json!(model_inputs));
    for (index, wave) in cases.iter().enumerate() {
        let mut candidate =
            ClientCreateSignatures::new(&storage::Crdts::default(), &doc, &signer).unwrap();
        assert_eq!(
            candidate.respond(wave).is_ok(),
            expectations[index]["clientSigns"].as_bool().unwrap()
        );
    }
    let signatures = prepared.respond(&children).unwrap();
    assert_eq!(signatures.len(), children.len());
    let mut expected_links = Vec::new();
    for (bytes, signature) in children.iter().zip(signatures) {
        assert!(
            crypto::public_key_from_string(crypto::KeyType::Ed25519, &signer.public_key_hex)
                .unwrap()
                .verify(bytes, &signature)
                .unwrap()
        );
        let mut block = Block::from_dag_cbor(bytes).unwrap();
        let name = match &block.delta {
            CrdtDelta::Immutable(_) => IMMUTABLE_LINK_NAME.to_owned(),
            CrdtDelta::Lww(value) => value.field_name.clone(),
            other => panic!("unexpected field {other:?}"),
        };
        block.signature = Some(
            Signature::new(
                SignatureHeader::new(
                    SignatureType::EdDSA,
                    signer.public_key_hex.as_bytes().to_vec(),
                ),
                signature,
            )
            .generate_cid()
            .unwrap(),
        );
        expected_links.push((block.generate_cid().unwrap().to_string(), name));
    }
    expected_links.sort();
    let composite = prepared.composite_block().to_vec();
    let mut composite_block = Block::from_dag_cbor(&composite).unwrap();
    let links = composite_block.links.as_ref().unwrap();
    let observed_links = links
        .iter()
        .map(|link| (link.link.to_string(), link.name.clone()))
        .collect::<Vec<_>>();
    let render = |links: &[(String, String)]| {
        links
            .iter()
            .map(|(cid, name)| format!("{cid}:{name}"))
            .collect::<Vec<_>>()
    };
    let model_links = model(
        "ClientCreateGraph.lean",
        &json!([{
            "fields": intended_fields(&doc), "expectedLinks": render(&expected_links), "offeredLinks": render(&observed_links)
        }]),
    );
    assert!(model_links[0]["acceptsComposite"].as_bool().unwrap());
    composite_block.links.as_mut().unwrap().pop();
    let altered = composite_block.to_dag_cbor().unwrap();
    let altered_links = composite_block
        .links
        .as_ref()
        .unwrap()
        .iter()
        .map(|link| (link.link.to_string(), link.name.clone()))
        .collect::<Vec<_>>();
    let altered_model = model(
        "ClientCreateGraph.lean",
        &json!([{
            "fields": intended_fields(&doc), "expectedLinks": render(&expected_links), "offeredLinks": render(&altered_links)
        }]),
    );
    assert_eq!(
        prepared.respond(&[altered]).is_ok(),
        altered_model[0]["acceptsComposite"].as_bool().unwrap()
    );
    assert_eq!(prepared.respond(&[composite]).unwrap().len(), 1);
    assert!(prepared.respond(&children).is_err());
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn trusted_update_graph_matches_model_and_rejects_unapproved_waves() {
    use db::block::builder::{ClientUpdateSignatures, ClientUpdateSnapshot};

    let (_, signer) = fixture();
    let schema = CollectionVersion::new(
        "Settings",
        "settings-version",
        "settings",
        vec![
            FieldDescription::new("1", "name", FieldKind::string(), CType::LwwRegister),
            FieldDescription::new(
                "2",
                "tags",
                FieldKind::nillable_string_array(),
                CType::LwwRegister,
            ),
        ],
    );
    let initial = json!({"name":"before", "tags":["one"]});
    let mut create =
        ClientCreateSignatures::from_input(&storage::Crdts::default(), &schema, &initial, &signer)
            .unwrap();
    let child_bytes = create.child_blocks().to_vec();
    let child_sigs = create.respond(&child_bytes).unwrap();
    let signed = |bytes: &[u8], signature: Vec<u8>| {
        let mut block = Block::from_dag_cbor(bytes).unwrap();
        block.signature = Some(
            Signature::new(
                SignatureHeader::new(
                    SignatureType::EdDSA,
                    signer.public_key_hex.as_bytes().to_vec(),
                ),
                signature,
            )
            .generate_cid()
            .unwrap(),
        );
        block.to_dag_cbor().unwrap()
    };
    let mut history = child_bytes
        .iter()
        .zip(child_sigs)
        .map(|(bytes, sig)| signed(bytes, sig))
        .collect::<Vec<_>>();
    let composite = create.composite_block().to_vec();
    let sig = create
        .respond(std::slice::from_ref(&composite))
        .unwrap()
        .remove(0);
    let composite = signed(&composite, sig);
    let composite_cid = Block::from_dag_cbor(&composite)
        .unwrap()
        .generate_cid()
        .unwrap();
    history.push(composite);
    let doc_id = create.doc_id().to_string();
    let snapshot = ClientUpdateSnapshot::from_blocks(&doc_id, &history, &[composite_cid]).unwrap();
    let input = json!({"name":"after", "tags":null});
    let new_graph = || {
        ClientUpdateSignatures::from_input(
            &storage::Crdts::default(),
            &schema,
            &doc_id,
            &input,
            &snapshot,
            &signer,
        )
        .unwrap()
    };
    let graph = new_graph();
    let wave = vec![graph.composite_block().to_vec()];
    let mut changed = wave.clone();
    changed[0][0] ^= 1;
    let cases = [
        wave.clone(),
        changed,
        Vec::new(),
        vec![wave[0].clone(), wave[0].clone()],
    ];
    let blocks = wave.iter().map(hex::encode).collect::<Vec<_>>();
    let heads = vec![composite_cid.to_string()];
    let changes = vec!["name=after", "tags=null"];
    let intent = json!({"collection":"Settings", "document":doc_id, "version":"settings-version", "heads":heads, "changes":changes, "blocks":blocks});
    let inputs = cases.iter().map(|offered| json!({"intent":intent, "offered": {
        "collection":"Settings", "document":doc_id, "version":"settings-version", "heads":heads,
        "changes":changes, "blocks":offered.iter().map(hex::encode).collect::<Vec<_>>()
    }, "requestedImmutable":[], "changes":[{"name":"name","before":initial["name"].to_string(),"after":input["name"].to_string()},{"name":"tags","before":initial["tags"].to_string(),"after":input["tags"].to_string()}]})).collect::<Vec<_>>();
    let expected = model("ClientSignedUpdate.lean", &json!(inputs));
    for (index, offered) in cases.iter().enumerate() {
        assert_eq!(
            new_graph().respond(offered).is_ok(),
            expected[index]["signs"].as_bool().unwrap()
        );
    }
    let mut graph = new_graph();
    let changed_target = json!({"collection":"Settings", "document":"wrong-document", "version":"settings-version", "heads":heads, "changes":changes, "blocks":blocks});
    let target_expectation = model(
        "ClientSignedUpdate.lean",
        &json!([{"intent":intent, "offered":changed_target, "changes":[], "requestedImmutable":[]} ]),
    );
    assert!(graph.matches_input("Settings", &doc_id, &input));
    assert_eq!(
        graph.matches_input("Settings", "wrong-document", &input),
        target_expectation[0]["signs"].as_bool().unwrap()
    );
    assert!(!graph.matches_input("Settings", &doc_id, &json!({"name":"changed", "tags":null})));
    assert_eq!(graph.respond(&wave).unwrap().len(), 1);
    assert!(graph.respond(&wave).is_err());
    let wrong_id = document::DocID::new_v0_from_seed("wrong").to_string();
    assert!(ClientUpdateSnapshot::from_blocks(&wrong_id, &history, &[composite_cid]).is_err());
    let mut damaged_history = history.clone();
    damaged_history[0][0] ^= 1;
    assert!(
        ClientUpdateSnapshot::from_blocks(&doc_id, &damaged_history, &[composite_cid]).is_err()
    );

    let crdts = storage::Crdts::default();
    let mut before_doc = Document::new(schema.clone());
    for bytes in &child_bytes {
        let block = Block::from_dag_cbor(bytes).unwrap();
        let CrdtDelta::Lww(delta) = block.delta else {
            panic!("expected LWW")
        };
        let field = schema.field_by_name(&delta.field_name).unwrap();
        let crdt = crdts.field(field.crdt_type).unwrap();
        let state = crdt
            .block_state(field, delta.priority, &delta.data)
            .unwrap();
        let value = crdt.materialize(state.part()).unwrap();
        if !value.is_nil() {
            before_doc.load(field.name.clone(), value);
        }
    }
    for patch in [
        json!({"name":"before"}),
        json!({"name":"before", "tags":["one"]}),
        json!({"name":"before", "tags":null}),
    ] {
        let fields = patch
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let update = query::plan::mutation::UpdateInput {
            fields,
            ops: rapidhash::RapidHashMap::default(),
        };
        let mut after_doc = Document::new(schema.clone());
        update
            .apply_to(&crdts, &mut after_doc, Some(&schema))
            .unwrap();
        let observations = patch
            .as_object()
            .unwrap()
            .keys()
            .map(|name| {
                let before = before_doc.get(name).map(|value| format!("{value:?}"));
                let after = format!("{:?}", after_doc.get(name).unwrap());
                json!({"name":name, "before":before, "after":after})
            })
            .collect::<Vec<_>>();
        let modeled = model(
            "ClientSignedUpdate.lean",
            &json!([{"intent":intent, "offered":intent, "changes":observations, "requestedImmutable":[]}]),
        );
        let mut expected_fields = modeled[0]["writtenFields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let candidate = ClientUpdateSignatures::from_input(
            &storage::Crdts::default(),
            &schema,
            &doc_id,
            &patch,
            &snapshot,
            &signer,
        )
        .unwrap();
        let mut observed_fields = if candidate.is_complete() {
            Vec::new()
        } else {
            Block::from_dag_cbor(candidate.composite_block())
                .unwrap()
                .links
                .unwrap_or_default()
                .into_iter()
                .map(|link| link.name)
                .collect::<Vec<_>>()
        };
        expected_fields.sort();
        observed_fields.sort();
        assert_eq!(observed_fields, expected_fields);
        assert_eq!(candidate.is_complete(), expected_fields.is_empty());
    }
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn immutable_genesis_is_verified_and_immutable_updates_are_refused() {
    use db::block::builder::{ClientUpdateSignatures, ClientUpdateSnapshot};
    let (doc, signer) = fixture();
    let schema = doc.collection().clone();
    let mut create =
        ClientCreateSignatures::new(&storage::Crdts::default(), &doc, &signer).unwrap();
    let children = create.child_blocks().to_vec();
    let signatures = create.respond(&children).unwrap();
    let mut history = Vec::new();
    for (bytes, signature) in children.iter().zip(signatures) {
        let mut block = Block::from_dag_cbor(bytes).unwrap();
        let signature = Signature::new(
            SignatureHeader::new(
                SignatureType::EdDSA,
                signer.public_key_hex.as_bytes().to_vec(),
            ),
            signature,
        );
        block.signature = Some(signature.generate_cid().unwrap());
        if matches!(block.delta, CrdtDelta::Immutable(_)) {
            history.push(signature.to_dag_cbor().unwrap());
        }
        history.push(block.to_dag_cbor().unwrap());
    }
    let composite = create.composite_block().to_vec();
    let signature = create
        .respond(std::slice::from_ref(&composite))
        .unwrap()
        .remove(0);
    let mut block = Block::from_dag_cbor(&composite).unwrap();
    block.signature = Some(
        Signature::new(
            SignatureHeader::new(
                SignatureType::EdDSA,
                signer.public_key_hex.as_bytes().to_vec(),
            ),
            signature,
        )
        .generate_cid()
        .unwrap(),
    );
    let head = block.generate_cid().unwrap();
    history.push(block.to_dag_cbor().unwrap());
    let snapshot = ClientUpdateSnapshot::from_blocks(create.doc_id(), &history, &[head]).unwrap();
    let patches = [
        json!({"display_name":"changed"}),
        json!({"agent_did":"did:key:other"}),
        json!({"node_did":"did:key:other"}),
    ];
    let intent = json!({"collection":"Agent", "document":create.doc_id(), "version":schema.version_id, "heads":[], "changes":[], "blocks":[]});
    let inputs = patches.iter().map(|patch| json!({"intent":intent,"offered":intent,"changes":[],"requestedImmutable":patch.as_object().unwrap().keys().map(|name| schema.field_by_name(name).unwrap().immutable).collect::<Vec<_>>() })).collect::<Vec<_>>();
    let expected = model("ClientSignedUpdate.lean", &json!(inputs));
    for (index, patch) in patches.iter().enumerate() {
        assert_eq!(
            ClientUpdateSignatures::from_input(
                &storage::Crdts::default(),
                &schema,
                create.doc_id(),
                patch,
                &snapshot,
                &signer
            )
            .is_ok(),
            expected[index]["fieldsAllowed"].as_bool().unwrap()
        );
    }
    let missing = history
        .iter()
        .filter(|bytes| Signature::from_dag_cbor(bytes).is_err())
        .cloned()
        .collect::<Vec<_>>();
    assert!(ClientUpdateSnapshot::from_blocks(create.doc_id(), &missing, &[head]).is_err());
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn trusted_delete_graph_matches_model_and_refuses_other_waves() {
    use db::block::builder::{ClientDeleteSignatures, ClientUpdateSnapshot};
    let (_, signer) = fixture();
    let schema = CollectionVersion::new(
        "Settings",
        "settings-version",
        "settings",
        vec![FieldDescription::new(
            "1",
            "name",
            FieldKind::string(),
            CType::LwwRegister,
        )],
    );
    let mut create = ClientCreateSignatures::from_input(
        &storage::Crdts::default(),
        &schema,
        &json!({"name":"before"}),
        &signer,
    )
    .unwrap();
    let children = create.child_blocks().to_vec();
    let signatures = create.respond(&children).unwrap();
    let signed = |bytes: &[u8], signature: Vec<u8>| {
        let mut block = Block::from_dag_cbor(bytes).unwrap();
        block.signature = Some(
            Signature::new(
                SignatureHeader::new(
                    SignatureType::EdDSA,
                    signer.public_key_hex.as_bytes().to_vec(),
                ),
                signature,
            )
            .generate_cid()
            .unwrap(),
        );
        block.to_dag_cbor().unwrap()
    };
    let mut history = children
        .iter()
        .zip(signatures)
        .map(|(bytes, sig)| signed(bytes, sig))
        .collect::<Vec<_>>();
    let composite = create.composite_block().to_vec();
    let signature = create
        .respond(std::slice::from_ref(&composite))
        .unwrap()
        .remove(0);
    let composite = signed(&composite, signature);
    let head = Block::from_dag_cbor(&composite)
        .unwrap()
        .generate_cid()
        .unwrap();
    history.push(composite);
    let doc_id = create.doc_id().to_owned();
    let snapshot = ClientUpdateSnapshot::from_blocks(&doc_id, &history, &[head]).unwrap();
    // The model derives acceptance from the exact intent and authenticated graph.
    let patch = json!({"name":"after"});
    let update = db::block::builder::ClientUpdateSignatures::from_input(
        &storage::Crdts::default(), &schema, &doc_id, &patch, &snapshot, &signer,
    ).unwrap();
    let delete = ClientDeleteSignatures::from_snapshot(&schema, &doc_id, &snapshot, &signer).unwrap();
    let mut cases = Vec::new();
    for (operation, evidence) in [("update", serde_json::to_value(update.evidence()).unwrap()),
                                   ("delete", serde_json::to_value(delete.evidence()).unwrap())] {
        let mut tampered = evidence.clone();
        let byte = tampered["composite_signature"][0].as_u64().unwrap();
        tampered["composite_signature"][0] = json!(byte ^ 1);
        cases.push((operation, evidence, true));
        cases.push((operation, tampered, false));
    }
    let modeled = model("VerifiedMutationHandoff.lean", &json!(cases.iter().map(|(op, _, valid)| {
        let graph = json!({"signer":"user", "operation":op, "collection":"Settings", "document":doc_id,
            "input":patch.to_string(), "history":head.to_string(), "blocks":["composite"]});
        json!({"approved":graph, "offered":graph, "claimedActor":"user", "verified":valid,
            "nativeAuthorized":true, "waveMatches":true, "commit":true})
    }).collect::<Vec<_>>()));
    for ((operation, wire, _), expected) in cases.into_iter().zip(modeled.as_array().unwrap()) {
        let evidence = serde_json::from_value(wire).unwrap();
        let result = match operation {
            "update" => db::block::builder::VerifiedClientMutation::verify_update(
                &storage::Crdts::default(), &schema, &doc_id, &patch, &snapshot, evidence),
            _ => db::block::builder::VerifiedClientMutation::verify_delete(
                &schema, &doc_id, &snapshot, evidence),
        };
        assert_eq!(result.is_ok(), expected["accepts"].as_bool().unwrap());
    }
    let new_graph =
        || ClientDeleteSignatures::from_snapshot(&schema, &doc_id, &snapshot, &signer).unwrap();
    let wave = vec![new_graph().composite_block().to_vec()];
    let block = Block::from_dag_cbor(&wave[0]).unwrap();
    let CrdtDelta::Composite(delta) = block.delta else {
        panic!("expected composite")
    };
    assert_eq!(delta.status, 2);
    let mut changed = wave.clone();
    changed[0][0] ^= 1;
    let waves = [
        wave.clone(),
        changed,
        vec![],
        vec![wave[0].clone(), wave[0].clone()],
    ];
    let intent = json!({"collection":"Settings", "document":doc_id, "version":"settings-version", "heads":[head.to_string()], "priority":delta.priority, "block":hex::encode(&wave[0]), "signer":"caller"});
    let inputs = waves.iter().map(|offered| {
        let mut next = intent.clone();
        next["block"] = json!(if offered.len() == 1 {hex::encode(&offered[0])} else {"not-one-block".to_owned()});
        json!({"approved":intent, "offered":next, "authenticated":"caller", "snapshotValid":true, "authorized":true, "committed":true, "responseReceived":true})
    }).collect::<Vec<_>>();
    let expected = model("ClientSignedDelete.lean", &json!(inputs));
    for (offered, expected) in waves.iter().zip(expected.as_array().unwrap()) {
        assert_eq!(
            new_graph().respond(offered).is_ok(),
            expected["signs"].as_bool().unwrap()
        );
    }
    let mut graph = new_graph();
    assert!(graph.matches_target("Settings", &doc_id));
    assert!(!graph.matches_target("Settings", "different"));
    assert_eq!(graph.respond(&wave).unwrap().len(), 1);
    assert!(graph.respond(&wave).is_err());
    let mut wrong_schema = schema.clone();
    wrong_schema.fields.clear();
    let invalid = json!({"approved":intent, "offered":intent, "authenticated":"caller", "snapshotValid":false, "authorized":true, "committed":true, "responseReceived":true});
    let modeled = model("ClientSignedDelete.lean", &json!([invalid]));
    assert_eq!(
        ClientDeleteSignatures::from_snapshot(&wrong_schema, &doc_id, &snapshot, &signer).is_ok(),
        modeled[0]["signs"].as_bool().unwrap()
    );
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn signed_create_evidence_matches_the_modeled_exact_handoff() {
    use db::block::builder::{ClientCreateEvidence, VerifiedClientCreate};
    let (doc, signer) = fixture();
    let input = serde_json::to_value(doc.to_map().unwrap()).unwrap();
    let graph = ClientCreateSignatures::new(&storage::Crdts::default(), &doc, &signer).unwrap();
    let evidence = graph.evidence();
    let wire = serde_json::to_value(&evidence).unwrap();
    assert!(wire.get("private_key_bytes").is_none());
    let check = |evidence: ClientCreateEvidence, input: &Value| {
        VerifiedClientCreate::verify(
            &storage::Crdts::default(),
            doc.collection(),
            input,
            evidence,
        )
    };
    let expected = json!({"signer":"user", "collection":"Agent", "document":graph.doc_id(), "input":input.to_string(), "blocks":["exact"]});
    let mut cases = Vec::new();
    let mut altered = wire.clone();
    let value = altered["child_signatures"][0][0].as_u64().unwrap();
    altered["child_signatures"][0][0] = json!(value ^ 1);
    cases.push((wire.clone(), input.clone(), true));
    cases.push((altered, input.clone(), false));
    let mut changed_input = input.clone();
    changed_input["display_name"] = json!("different");
    cases.push((wire.clone(), changed_input, false));
    let inputs = cases
        .iter()
        .map(|(_, _, valid)| {
            json!({"approved":expected, "offered":expected,
        "claimedActor":"user", "verified":valid, "nativeAuthorized":true, "commit":true})
        })
        .collect::<Vec<_>>();
    let modeled = model("VerifiedCreateHandoff.lean", &json!(inputs));
    for ((wire, input, _), expected) in cases.into_iter().zip(modeled.as_array().unwrap()) {
        let evidence = serde_json::from_value(wire).unwrap();
        assert_eq!(
            check(evidence, &input).is_ok(),
            expected["accepts"].as_bool().unwrap()
        );
    }
}
