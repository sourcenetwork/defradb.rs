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
