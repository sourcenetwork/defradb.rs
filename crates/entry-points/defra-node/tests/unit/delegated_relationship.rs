use crypto::Key;
use defra_core::RequestContext;
use identity::{Did, Identity as _, RawIdentity};

use super::Node;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const READER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const POLICY: &str = "name: Rows\nresources:\n  - name: rows\n    relations:\n      - name: reader\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n";

fn actor(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

async fn fixture() -> (Node, String, String) {
    let key = crypto::generate_ed25519().unwrap();
    let identity = RawIdentity::from_ed25519(key.clone()).unwrap();
    let node_did = identity.did().unwrap().to_string();
    let public = identity.public_key_bytes();
    defra_core::signing::store_identity(
        &node_did,
        defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: key.raw().to_vec(),
            public_key_hex: hex::encode(&public),
            public_key_bytes: public,
            remote_signer: None,
            signing_authorization: None,
        },
    )
    .expect("the store takes the identity");
    let node = Node::builder()
        .with_node_identity_did(&node_did)
        .build()
        .await
        .unwrap();
    let policy = node
        .with_identity(identity::Did::new(OWNER).unwrap())
        .add_dac_policy(POLICY, None)
        .await
        .unwrap();
    node.with_identity(identity::Did::new(OWNER).unwrap())
        .add_collection(&format!(
            "type Record @policy(id: \"{policy}\", resource: \"rows\") {{ pending: Boolean }}"
        ))
        .await
        .unwrap();
    let response = node
        .execute_request(
            query::QueryRequest::new("mutation { add_Record(input: {pending: true}) { _docID } }")
                .with_identity(Some(Did::new(OWNER).unwrap())),
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let doc = response.data.unwrap()["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    (node, node_did, doc)
}

async fn visible(node: &Node) -> usize {
    let response = node
        .execute_request(
            query::QueryRequest::new("{ Record { pending } }")
                .with_identity(Some(Did::new(READER).unwrap())),
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["Record"].as_array().unwrap().len()
}

#[test]
fn node_owned_transaction_keeps_the_selected_actor_and_finality() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let (node, node_did, doc) = fixture().await;
        assert_ne!(node_did, OWNER);
        let owner = actor(OWNER);
        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        assert!(node.core().add_dac_actor_relationship_for(handle, &RequestContext::default(), "Record", &doc, "reader", READER).await.is_err());
        assert!(node.with_transaction(handle).acting_for(identity::Did::new(READER).unwrap()).add_dac_actor_relationship("Record", &doc, "reader", READER).await.is_err());
        assert!(node.core().add_dac_actor_relationship_for(handle, &owner.clone().with_authorization_expiration(Some(0)), "Record", &doc, "reader", READER).await.is_err());
        let mismatched = owner.clone().with_txn(Some(defra_core::TxnId {
            start: 0,
            route: u64::MAX,
        }));
        assert!(node.core().add_dac_actor_relationship_for(handle, &mismatched, "Record", &doc, "reader", READER).await.is_err());
        let mutation = format!("mutation {{ update_Record(docID: \"{doc}\", filter: {{ pending: {{ _eq: true }} }}, input: {{ pending: false }}) {{ _docID }} }}");
        let response = node.with_transaction(handle).execute_request(query::QueryRequest::new(mutation).with_identity(Some(Did::new(OWNER).unwrap()))).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(response.data.unwrap()["update_Record"].as_array().unwrap().len(), 1);
        assert!(!node.with_transaction(handle).acting_for(identity::Did::new(OWNER).unwrap()).add_dac_actor_relationship("Record", &doc, "reader", READER).await.unwrap());
        assert_eq!(visible(&node).await, 0);
        transaction.commit().await.unwrap();
        assert_eq!(visible(&node).await, 1);

        let transaction = node.new_txn(false).await.unwrap();
        assert!(node.with_transaction(transaction.id()).acting_for(identity::Did::new(OWNER).unwrap()).delete_dac_actor_relationship("Record", &doc, "reader", READER).await.unwrap());
        transaction.discard().await.unwrap();
        assert_eq!(visible(&node).await, 1);

        let transaction = node.new_txn(false).await.unwrap();
        assert!(node.with_transaction(transaction.id()).acting_for(identity::Did::new(OWNER).unwrap()).delete_dac_actor_relationship("Record", &doc, "reader", READER).await.unwrap());
        transaction.commit().await.unwrap();
        assert_eq!(visible(&node).await, 0);

        let foreign = node.runner().begin_txn_as(&owner, false).await.unwrap();
        assert!(node.with_transaction(foreign.parse::<defra_core::TxnId>().unwrap()).acting_for(identity::Did::new(OWNER).unwrap()).add_dac_actor_relationship("Record", &doc, "reader", READER).await.is_err());
        node.runner().discard_txn_as(&owner, &foreign).await.unwrap();
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn selected_actor_nac_revocation_conflicts_with_node_owned_grant() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let (node, node_did, doc) = fixture().await;
        let manager = node.nac_manager().clone();
        let owner_did = Did::new(&node_did).unwrap();
        node.on_acp(move || async move {
            manager
                .enable(&owner_did)
                .await
                .map_err(crate::Error::NacInit)?;
            Ok(())
        })
        .await
        .unwrap();
        node.with_identity(Did::new(&node_did).unwrap())
            .add_nac_actor_relationship(acp::NodePermission::CollectionGet.as_str(), OWNER)
            .await
            .unwrap();
        let tx = node.new_txn(false).await.unwrap();
        assert!(node
            .with_transaction(tx.id())
            .acting_for(identity::Did::new(OWNER).unwrap())
            .add_dac_actor_relationship("Record", &doc, "reader", READER)
            .await
            .is_err());
        tx.discard().await.unwrap();

        node.with_identity(identity::Did::new(&node_did).unwrap())
            .add_nac_actor_relationship(acp::NodePermission::DacRelationAdd.as_str(), OWNER)
            .await
            .unwrap();
        let tx = node.new_txn(false).await.unwrap();
        node.with_transaction(tx.id())
            .acting_for(identity::Did::new(OWNER).unwrap())
            .add_dac_actor_relationship("Record", &doc, "reader", READER)
            .await
            .unwrap();
        node.with_identity(identity::Did::new(&node_did).unwrap())
            .delete_nac_actor_relationship(acp::NodePermission::DacRelationAdd.as_str(), OWNER)
            .await
            .unwrap();
        assert!(tx.commit().await.is_err());
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn embedded_delegation_matches_executed_lean_model() {
    use serde_json::{json, Value};
    use std::io::Write;
    use std::process::{Command, Stdio};

    let cases = workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let (node, _, doc) = fixture().await;
        let stages_owed = node.document_acp().stages_relationship_changes();
        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        let mut cases = Vec::<Value>::new();
        for (context, authenticated, dac) in [
            (RequestContext::default(), false, true),
            (actor(READER), true, false),
            (actor(OWNER), true, true),
        ] {
            let accepted = node
                .core()
                .add_dac_actor_relationship_for(handle, &context, "Record", &doc, "reader", READER)
                .await
                .is_ok();
            cases.push(json!({
                "input": {
                    "trustedEmbedder": true, "ownsTransaction": true,
                    "authenticatedActor": authenticated, "actorNac": true,
                    "actorDac": dac, "boundProvider": true,
                    "stagesOwed": stages_owed
                },
                "mayStage": accepted
            }));
        }
        transaction.discard().await.unwrap();
        let context = actor(OWNER);
        let foreign = node.runner().begin_txn_as(&context, false).await.unwrap();
        let accepted = node
            .with_transaction(foreign.parse::<defra_core::TxnId>().unwrap())
            .acting_for(identity::Did::new(OWNER).unwrap())
            .add_dac_actor_relationship("Record", &doc, "reader", READER)
            .await
            .is_ok();
        cases.push(json!({
            "input": {
                "trustedEmbedder": true, "ownsTransaction": false,
                "authenticatedActor": true, "actorNac": true,
                "actorDac": true, "boundProvider": true,
                "stagesOwed": stages_owed
            },
            "mayStage": accepted
        }));
        node.runner()
            .discard_txn_as(&context, &foreign)
            .await
            .unwrap();
        node.shutdown().await.unwrap();
        cases
    })
    .unwrap();
    #[cfg(feature = "vera")]
    let cases = [cases, owed_staging_cases()].concat();
    let inputs: Vec<_> = cases.iter().map(|case| case["input"].clone()).collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../proofs/lean");
    let mut child = Command::new("lake")
        .current_dir(root)
        .args(["env", "lean", "--run", "DelegatedRelationship.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases, expected);
}

/// The embedder's call on a remote provider that cannot join the transaction
/// and stages the change in it as owed: accepted for the actor that may make
/// the change, refused for one the provider's document check refuses.
#[cfg(feature = "vera")]
fn owed_staging_cases() -> Vec<serde_json::Value> {
    use crate::staged_relationship_tests::{node_identity, provision, start, Remote};
    use serde_json::json;

    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        // The node holds the reader's key too, so the reader's change is
        // refused by the provider's document check and not for want of
        // authority to send it.
        remote.keys.insert(READER.to_owned(), ());
        let node = start(&node_identity(), Some(&remote), None).await;
        provision(&node).await;
        let created = node
            .execute_request(
                query::QueryRequest::new(
                    "mutation { add_Record(input: {name: \"owed\"}) { _docID } }",
                )
                .with_identity(Some(Did::new(OWNER).unwrap())),
            )
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc = created.data.unwrap()["add_Record"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        let provider = node.document_acp();
        let bound = provider.supports_transactions();
        let stages_owed = provider.stages_relationship_changes();
        let transaction = node.new_txn(false).await.unwrap();
        let mut cases = Vec::new();
        for (caller, dac) in [(READER, false), (OWNER, true)] {
            let change = node
                .with_transaction(transaction.id())
                .acting_for(identity::Did::new(caller).unwrap())
                .change_dac_actor_relationship("Record", &doc, "reader", READER, false)
                .await;
            if let Ok(change) = &change {
                assert!(
                    matches!(change, super::RelationshipChange::Staged { .. }),
                    "{change:?}"
                );
            }
            cases.push(json!({
                "input": {
                    "trustedEmbedder": true, "ownsTransaction": true,
                    "authenticatedActor": true, "actorNac": true,
                    "actorDac": dac, "boundProvider": bound,
                    "stagesOwed": stages_owed
                },
                "mayStage": change.is_ok()
            }));
        }
        transaction.discard().await.unwrap();
        node.shutdown().await.unwrap();
        cases
    })
    .unwrap()
}
