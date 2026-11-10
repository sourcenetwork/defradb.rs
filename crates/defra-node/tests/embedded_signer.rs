use crypto::Key;
use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_node::{EmbeddedSigner, Node, QueryRequest};
use identity::{Did, Identity as _, RawIdentity};

fn key() -> (String, SigningConfig) {
    let private = crypto::generate_ed25519().unwrap();
    let identity = RawIdentity::from_ed25519(private.clone()).unwrap();
    let public = identity.public_key_bytes();
    (
        identity.did().unwrap().to_string(),
        SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: private.raw().to_vec(),
            public_key_hex: hex::encode(&public),
            public_key_bytes: public,
            remote_signer: None,
            signing_authorization: None,
        },
    )
}

fn request(query: impl Into<String>, actor: &str) -> QueryRequest {
    QueryRequest::new(query.into()).with_identity(Some(Did::new(actor).unwrap()))
}

#[test]
fn selected_actor_signs_create_update_and_respects_transaction_finality() {
    workers::block_on(async {
        let (node_did, node_key) = key();
        let (actor, actor_key) = key();
        let (other, other_key) = key();
        defra_core::signing::store_identity(&node_did, node_key).expect("the store takes the identity");
        let signer = EmbeddedSigner::from_config(actor_key).unwrap();
        let other_signer = EmbeddedSigner::from_config(other_key).unwrap();
        let node = Node::builder().with_node_identity_did(&node_did).build().await.unwrap();
        node.add_collection("type SignedFact { value: String }").await.unwrap();
        let create = node.execute_request(signer.attach(request(r#"mutation { add_SignedFact(input: {value: "one"}) { _docID } }"#, &actor))).await;
        assert!(!create.has_errors(), "{:?}", create.errors);
        let doc = create.data.unwrap()["add_SignedFact"][0]["_docID"].as_str().unwrap().to_owned();
        let commits_query = format!(r#"{{ _commits(docID: "{doc}", filter: {{fieldName: {{_eq: "_C"}}}}) {{ cid }} }}"#);
        let commits = node.execute(&commits_query).await;
        let cid = commits.data.unwrap()["_commits"][0]["cid"].as_str().unwrap().to_owned();
        assert_eq!(node.verified_block_signer_did(&cid).await.unwrap(), actor);

        let update = format!(r#"mutation {{ update_SignedFact(docID: "{doc}", input: {{value: "two"}}) {{ _docID }} }}"#);
        let tx = node.begin_transaction_guard(false).await.unwrap();
        let result = node.execute_request_in_txn(signer.attach(request(&update, &actor)), tx.handle()).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let commits = node.execute_request_in_txn(QueryRequest::new(&commits_query), tx.handle()).await;
        let cids: Vec<String> = commits.data.unwrap()["_commits"].as_array().unwrap().iter().map(|c| c["cid"].as_str().unwrap().to_owned()).collect();
        let updated = cids.iter().find(|candidate| **candidate != cid).unwrap();
        assert_eq!(node.verified_block_signer_did_in_txn(updated, tx.handle()).await.unwrap(), actor);
        let other_create = node.execute_request_in_txn(other_signer.attach(request(r#"mutation { add_SignedFact(input: {value: "other"}) { _docID } }"#, &other)), tx.handle()).await;
        assert!(!other_create.has_errors(), "{:?}", other_create.errors);
        let other_doc = other_create.data.unwrap()["add_SignedFact"][0]["_docID"].as_str().unwrap().to_owned();
        let other_query = format!(r#"{{ _commits(docID: "{other_doc}", filter: {{fieldName: {{_eq: "_C"}}}}) {{ cid }} }}"#);
        let other_commits = node.execute_request_in_txn(QueryRequest::new(other_query), tx.handle()).await;
        let other_cid = other_commits.data.unwrap()["_commits"][0]["cid"].as_str().unwrap().to_owned();
        assert_eq!(node.verified_block_signer_did_in_txn(&other_cid, tx.handle()).await.unwrap(), other);
        tx.commit().await.unwrap();
        assert_eq!(node.verified_block_signer_did(updated).await.unwrap(), actor);
        assert_eq!(node.verified_block_signer_did(&other_cid).await.unwrap(), other);

        let tx = node.begin_transaction_guard(false).await.unwrap();
        let discard = node.execute_request_in_txn(signer.attach(request(update.replace("two", "three"), &actor)), tx.handle()).await;
        assert!(!discard.has_errors(), "{:?}", discard.errors);
        tx.discard().await.unwrap();
        let rows = node.execute("{ SignedFact { value } }").await;
        let rows = rows.data.unwrap();
        let rows = rows["SignedFact"].as_array().unwrap();
        assert!(rows.iter().any(|row| row["value"] == "two"));
        assert!(!rows.iter().any(|row| row["value"] == "three"));

        let mismatched = node.execute_request(signer.attach(request(&update, &other))).await;
        assert!(mismatched.has_errors());
        let missing = node.execute_request(signer.attach(QueryRequest::new(&update))).await;
        assert!(missing.has_errors());
        let wrong_signer = node.execute_request(other_signer.attach(request(&update, &actor))).await;
        assert!(wrong_signer.has_errors());
        let foreign_context = defra_core::RequestContext::default().with_identity(Some(other));
        let foreign = node.runner().begin_txn_as(&foreign_context, false).await.unwrap();
        let denied = node.execute_request_in_txn(signer.attach(request(&update, &actor)), &foreign).await;
        assert!(denied.has_errors());
        node.runner().discard_txn_as(&foreign_context, &foreign).await.unwrap();
        node.shutdown().await.unwrap();
    }).unwrap();
}

/// A caller's key travels on the request context, so a route that carries a
/// context signs with it just as a request with the key attached does.
#[test]
fn a_context_carrying_a_callers_key_signs_as_that_actor() {
    workers::block_on(async {
        let (node_did, node_key) = key();
        let (actor, actor_key) = key();
        defra_core::signing::store_identity(&node_did, node_key)
            .expect("the store takes the identity");
        let node = Node::builder()
            .with_node_identity_did(&node_did)
            .build()
            .await
            .unwrap();
        node.add_collection("type SignedFact { value: String }")
            .await
            .unwrap();
        let context = defra_core::RequestContext::default()
            .with_identity(Some(actor.clone()))
            .with_signing(Some(actor_key));
        let create = node
            .execute_request_as(
                &context,
                QueryRequest::new(
                    r#"mutation { add_SignedFact(input: {value: "one"}) { _docID } }"#,
                ),
            )
            .await
            .unwrap();
        assert!(!create.has_errors(), "{:?}", create.errors);
        let doc = create.data.unwrap()["add_SignedFact"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        let commits = node
            .execute(&format!(
                r#"{{ _commits(docID: "{doc}", filter: {{fieldName: {{_eq: "_C"}}}}) {{ cid }} }}"#
            ))
            .await;
        let cid = commits.data.unwrap()["_commits"][0]["cid"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(node.verified_block_signer_did(&cid).await.unwrap(), actor);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn invalid_or_missing_local_key_is_rejected() {
    let (_, mut missing) = key();
    missing.private_key_bytes.clear();
    assert!(EmbeddedSigner::from_config(missing).is_err());
    let (_, mut inconsistent) = key();
    let (_, another) = key();
    inconsistent.public_key_bytes = another.public_key_bytes.clone();
    assert!(EmbeddedSigner::from_config(inconsistent).is_err());
    let (_, mut inconsistent_hex) = key();
    inconsistent_hex.public_key_hex = "00".into();
    assert!(EmbeddedSigner::from_config(inconsistent_hex).is_err());
    let (_, mut malformed) = key();
    malformed.private_key_bytes[0] ^= 1;
    assert!(EmbeddedSigner::from_config(malformed).is_err());
    let (_, mut unsupported) = key();
    unsupported.key_type = SigningKeyType::Secp256k1;
    assert!(EmbeddedSigner::from_config(unsupported).is_err());
}

#[test]
fn selected_signer_does_not_bypass_document_acp() {
    workers::block_on(async {
        let (node_did, node_key) = key();
        let (owner, owner_key) = key();
        let (other, other_key) = key();
        defra_core::signing::store_identity(&node_did, node_key).expect("the store takes the identity");
        let node = Node::builder().with_node_identity_did(&node_did).build().await.unwrap();
        let policy = node.add_dac_policy_as(
            &defra_core::RequestContext::default().with_identity(Some(owner.clone())),
            "name: Rows\nresources:\n  - name: rows\n    relations:\n      - name: reader\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n",
        ).await.unwrap();
        node.add_collection(&format!("type Protected @policy(id: \"{policy}\", resource: \"rows\") {{ value: String }}")).await.unwrap();
        let signer = EmbeddedSigner::from_config(owner_key).unwrap();
        let other_signer = EmbeddedSigner::from_config(other_key).unwrap();
        let create = node.execute_request(signer.attach(request(r#"mutation { add_Protected(input: {value: "one"}) { _docID } }"#, &owner))).await;
        assert!(!create.has_errors(), "{:?}", create.errors);
        let doc = create.data.unwrap()["add_Protected"][0]["_docID"].as_str().unwrap().to_owned();
        let mutation = format!(r#"mutation {{ update_Protected(docID: "{doc}", input: {{value: "stolen"}}) {{ _docID }} }}"#);
        let denied = node.execute_request(other_signer.attach(request(mutation, &other))).await;
        assert!(denied.has_errors() || denied.data.as_ref().and_then(|data| data["update_Protected"].as_array()).is_some_and(Vec::is_empty));
        let bypass = defra_core::RequestContext::default().with_dac_bypass(true);
        let mutation = format!(r#"mutation {{ update_Protected(docID: "{doc}", input: {{value: "stolen"}}) {{ _docID }} }}"#);
        let rejected = node.execute_request(other_signer.attach(request(&mutation, &other).with_context(bypass.clone()))).await;
        assert!(rejected.has_errors(), "explicit signing must reject a DAC bypass request");
        let tx = node.begin_transaction_guard(false).await.unwrap();
        let rejected = node.execute_request_in_txn(other_signer.attach(request(&mutation, &other).with_context(bypass)), tx.handle()).await;
        assert!(rejected.has_errors(), "transactional explicit signing must reject a DAC bypass request");
        tx.discard().await.unwrap();
        let read = node.execute_request(request("{ Protected { value } }", &owner)).await;
        assert_eq!(read.data.unwrap()["Protected"][0]["value"], "one");
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn native_admission_matches_executed_lean_model() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    workers::block_on(async {
        let (node_did, node_key) = key();
        let (actor, actor_key) = key();
        let (other, _) = key();
        defra_core::signing::store_identity(&node_did, node_key)
            .expect("the store takes the identity");
        let node = Node::builder()
            .with_node_identity_did(&node_did)
            .build()
            .await
            .unwrap();
        node.add_collection("type ModelFact { value: String }")
            .await
            .unwrap();
        let signer = EmbeddedSigner::from_config(actor_key.clone()).unwrap();
        let inputs = serde_json::json!([
            {"actor": actor, "signer": actor, "keyValid": true, "bypass": false},
            {"actor": other, "signer": actor, "keyValid": true, "bypass": false},
            {"actor": null, "signer": actor, "keyValid": true, "bypass": false},
            {"actor": actor, "signer": actor, "keyValid": false, "bypass": false},
            {"actor": actor, "signer": actor, "keyValid": true, "bypass": true}
        ]);
        let mut lean = Command::new("lake")
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
            .args(["env", "lean", "--run", "EmbeddedSigner.lean"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(lean.stdin.take().unwrap(), "{inputs}").unwrap();
        let output = lean.wait_with_output().unwrap();
        assert!(output.status.success());
        let cases: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        for case in cases.as_array().unwrap() {
            if !case["input"]["keyValid"].as_bool().unwrap() {
                let mut invalid = actor_key.clone();
                invalid.private_key_bytes.clear();
                assert_eq!(
                    EmbeddedSigner::from_config(invalid).is_ok(),
                    case["admitted"].as_bool().unwrap()
                );
                continue;
            }
            let query = QueryRequest::new("{ ModelFact { value } }")
                .with_identity(
                    case["input"]["actor"]
                        .as_str()
                        .map(|did| Did::new(did).unwrap()),
                )
                .with_context(
                    defra_core::RequestContext::default()
                        .with_dac_bypass(case["input"]["bypass"].as_bool().unwrap()),
                );
            let result = node.execute_request(signer.attach(query)).await;
            assert_eq!(
                !result.has_errors(),
                case["admitted"].as_bool().unwrap(),
                "{:?}",
                result.errors
            );
        }
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// An identity-only node never signs for itself, but a caller who brings its
/// own key still signs as that actor: the node's signing choice covers only
/// the node's key.
#[test]
fn an_identity_only_node_signs_only_with_a_callers_key() {
    workers::block_on(async {
        let (node_did, node_key) = key();
        let (actor, actor_key) = key();
        defra_core::signing::store_identity(&node_did, node_key).expect("the store takes the identity");
        let signer = EmbeddedSigner::from_config(actor_key).unwrap();
        let node = Node::builder()
            .with_node_identity_did(&node_did)
            .with_identity_only()
            .build()
            .await
            .unwrap();
        node.add_collection("type SignedFact { value: String }")
            .await
            .unwrap();
        let composite_cid = |doc: String| {
            let node = &node;
            async move {
                let query = format!(
                    r#"{{ _commits(docID: "{doc}", filter: {{fieldName: {{_eq: "_C"}}}}) {{ cid }} }}"#
                );
                let commits = node.execute(&query).await;
                commits.data.unwrap()["_commits"][0]["cid"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            }
        };

        let signed = node
            .execute_request(signer.attach(request(
                r#"mutation { add_SignedFact(input: {value: "caller"}) { _docID } }"#,
                &actor,
            )))
            .await;
        assert!(!signed.has_errors(), "{:?}", signed.errors);
        let doc = signed.data.unwrap()["add_SignedFact"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        let cid = composite_cid(doc).await;
        assert_eq!(node.verified_block_signer_did(&cid).await.unwrap(), actor);

        let unsigned = node
            .execute(r#"mutation { add_SignedFact(input: {value: "node"}) { _docID } }"#)
            .await;
        assert!(!unsigned.has_errors(), "{:?}", unsigned.errors);
        let doc = unsigned.data.unwrap()["add_SignedFact"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        let cid = composite_cid(doc).await;
        let error = node.verified_block_signer_did(&cid).await.unwrap_err();
        assert!(error.to_string().contains("no signature"), "{error}");
        node.shutdown().await.unwrap();
    })
    .unwrap();
}
