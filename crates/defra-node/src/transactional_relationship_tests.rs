use defra_core::RequestContext;
use identity::Did;

use super::acp_ops::AcpOps;
use super::Node;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const READER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const POLICY: &str = "name: Rows\nresources:\n  - name: rows\n    relations:\n      - name: reader\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n";

fn actor(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

async fn fixture() -> (Node, String) {
    let node = Node::builder().build().await.unwrap();
    let policy = node.add_dac_policy_as(&actor(OWNER), POLICY).await.unwrap();
    node.add_collection(&format!(
        "type Record @policy(id: \"{policy}\", resource: \"rows\") {{ pending: Boolean }}"
    ))
    .await
    .unwrap();
    let result = node
        .execute_request(
            query::QueryRequest::new("mutation { add_Record(input: {pending: true}) { _docID } }")
                .with_identity(Some(Did::new(OWNER).unwrap())),
        )
        .await;
    assert!(!result.has_errors(), "{:?}", result.errors);
    let doc = result.data.unwrap()["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    (node, doc)
}

async fn visible(node: &Node) -> usize {
    let result = node
        .execute_request(
            query::QueryRequest::new("{ Record { pending } }")
                .with_identity(Some(Did::new(READER).unwrap())),
        )
        .await;
    assert!(!result.has_errors(), "{:?}", result.errors);
    result.data.unwrap()["Record"].as_array().unwrap().len()
}

#[test]
fn relationship_grant_and_revoke_obey_commit_and_discard() {
    workers::block_on(async {
        let (node, doc) = fixture().await;
        let context = actor(OWNER);
        let handle = node.runner().begin_txn_as(&context, false).await.unwrap();
        let tx = context
            .clone()
            .with_txn(Some(handle.as_str().parse().unwrap()));
        assert!(!node
            .add_dac_actor_relationship_as(&tx, "Record", &doc, "reader", READER)
            .await
            .unwrap());
        assert_eq!(visible(&node).await, 0);
        node.runner()
            .discard_txn_as(&context, &handle)
            .await
            .unwrap();
        assert_eq!(visible(&node).await, 0);

        let handle = node.runner().begin_txn_as(&context, false).await.unwrap();
        let tx = context
            .clone()
            .with_txn(Some(handle.as_str().parse().unwrap()));
        node.add_dac_actor_relationship_as(&tx, "Record", &doc, "reader", READER)
            .await
            .unwrap();
        assert_eq!(visible(&node).await, 0);
        node.runner()
            .commit_txn_as(&context, &handle)
            .await
            .unwrap();
        assert_eq!(visible(&node).await, 1);

        let handle = node.runner().begin_txn_as(&context, false).await.unwrap();
        let tx = context
            .clone()
            .with_txn(Some(handle.as_str().parse().unwrap()));
        assert!(node
            .delete_dac_actor_relationship_as(&tx, "Record", &doc, "reader", READER)
            .await
            .unwrap());
        assert_eq!(visible(&node).await, 1);
        node.runner()
            .commit_txn_as(&context, &handle)
            .await
            .unwrap();
        assert_eq!(visible(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn stale_marker_cannot_restore_revoked_relationship() {
    workers::block_on(async {
        let (node, doc) = fixture().await;
        let context = actor(OWNER);
        let handle = node.runner().begin_txn_as(&context, false).await.unwrap();
        let tx = context.clone().with_txn(Some(handle.as_str().parse().unwrap()));
        let mutation = format!(
            "mutation {{ update_Record(docID: \"{doc}\", filter: {{ pending: {{ _eq: true }} }}, input: {{ pending: false }}) {{ _docID }} }}"
        );
        let request = query::QueryRequest::new(mutation.clone())
            .with_identity(Some(Did::new(OWNER).unwrap()))
            .with_context(context.clone());
        let result = node.execute_request_in_txn(request, &handle).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        assert_eq!(result.data.unwrap()["update_Record"].as_array().unwrap().len(), 1);
        node.add_dac_actor_relationship_as(&tx, "Record", &doc, "reader", READER).await.unwrap();

        let other = node.execute_request(
            query::QueryRequest::new(mutation).with_identity(Some(Did::new(OWNER).unwrap()))
        ).await;
        assert!(!other.has_errors(), "{:?}", other.errors);
        node.add_dac_actor_relationship(OWNER, "Record", &doc, "reader", READER).await.unwrap();
        node.delete_dac_actor_relationship(OWNER, "Record", &doc, "reader", READER).await.unwrap();
        assert!(node.runner().commit_txn_as(&context, &handle).await.is_err());
        assert_eq!(visible(&node).await, 0);
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn wrong_actor_and_readonly_transactions_cannot_mutate_relationships() {
    workers::block_on(async {
        let (node, doc) = fixture().await;
        let owner = actor(OWNER);
        let handle = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let wrong = actor(READER).with_txn(Some(handle.as_str().parse().unwrap()));
        assert!(node
            .add_dac_actor_relationship_as(&wrong, "Record", &doc, "reader", READER)
            .await
            .is_err());
        node.runner().discard_txn_as(&owner, &handle).await.unwrap();
        let handle = node.runner().begin_txn_as(&owner, true).await.unwrap();
        let readonly = owner
            .clone()
            .with_txn(Some(handle.as_str().parse().unwrap()));
        assert!(node
            .add_dac_actor_relationship_as(&readonly, "Record", &doc, "reader", READER)
            .await
            .is_err());
        node.runner().discard_txn_as(&owner, &handle).await.unwrap();
        assert_eq!(visible(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn unbound_document_authorization_fails_without_writing() {
    workers::block_on(async {
        let (node, doc) = fixture().await;
        let unbound = std::sync::Arc::new(acp::LocalDocumentACP::new(std::sync::Arc::new(
            acp::MemoryAcpStore::new(),
        )));
        let ops = super::acp_ops::DbAcpOps::new(
            node.database.clone(),
            node.txn_registry.clone(),
            unbound,
            node.event_bus.clone(),
        );
        let owner = actor(OWNER);
        let handle = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let tx = owner
            .clone()
            .with_txn(Some(handle.as_str().parse().unwrap()));
        assert!(ops
            .add_dac_actor_relationship(&tx, OWNER, "Record", &doc, "reader", READER)
            .await
            .is_err());
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert_eq!(visible(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn revoking_node_relationship_authority_conflicts_with_pending_grant() {
    workers::block_on(async {
        let (node, doc) = fixture().await;
        let nac = node.nac_manager().clone();
        let nac_owner = Did::new(READER).unwrap();
        let writer = Did::new(OWNER).unwrap();
        let manager = nac.clone();
        let nac_owner_copy = nac_owner.clone();
        let writer_copy = writer.clone();
        node.on_acp(move || async move {
            manager
                .enable(&nac_owner_copy)
                .await
                .map_err(crate::Error::NacInit)?;
            manager
                .add_permission_grant(
                    &nac_owner_copy,
                    &writer_copy,
                    acp::NodePermission::DacRelationAdd,
                )
                .await
                .map_err(crate::Error::NacInit)?;
            Ok(())
        })
        .await
        .unwrap();
        let owner = actor(OWNER);
        let handle = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let tx = owner
            .clone()
            .with_txn(Some(handle.as_str().parse().unwrap()));
        node.add_dac_actor_relationship_as(&tx, "Record", &doc, "reader", READER)
            .await
            .unwrap();
        node.on_acp(move || async move {
            nac.remove_permission_grant(&nac_owner, &writer, acp::NodePermission::DacRelationAdd)
                .await
                .map_err(crate::Error::NacInit)?;
            Ok(())
        })
        .await
        .unwrap();
        assert!(node.runner().commit_txn_as(&owner, &handle).await.is_err());
        assert_eq!(visible(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn revoking_a_manager_conflicts_with_its_pending_relationship_grant() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        let policy = node.add_dac_policy_as(&actor(OWNER),
            "name: Rows\nresources:\n  - name: rows\n    relations:\n      - name: reader\n      - name: manager\n        manages: [reader]\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n").await.unwrap();
        node.add_collection(&format!("type Record @policy(id: \"{policy}\", resource: \"rows\") {{ pending: Boolean }}")).await.unwrap();
        let result = node.execute_request(
            query::QueryRequest::new("mutation { add_Record(input: {pending: true}) { _docID } }")
                .with_identity(Some(Did::new(OWNER).unwrap()))
        ).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let doc = result.data.unwrap()["add_Record"][0]["_docID"].as_str().unwrap().to_owned();
        node.add_dac_actor_relationship(OWNER, "Record", &doc, "manager", READER).await.unwrap();
        let manager = actor(READER);
        let handle = node.runner().begin_txn_as(&manager, false).await.unwrap();
        let tx = manager.clone().with_txn(Some(handle.as_str().parse().unwrap()));
        node.add_dac_actor_relationship_as(&tx, "Record", &doc, "reader", READER).await.unwrap();
        node.delete_dac_actor_relationship(OWNER, "Record", &doc, "manager", READER).await.unwrap();
        assert!(node.runner().commit_txn_as(&manager, &handle).await.is_err());
        assert_eq!(visible(&node).await, 0);
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn failed_relationship_statement_does_not_leak_its_writes_or_events() {
    workers::block_on(async {
        let (node, doc) = fixture().await;
        let mut updates = node.subscribe(&[events::EventName::Update]);
        let owner = actor(OWNER);
        let handle = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let tx = owner
            .clone()
            .with_txn(Some(handle.as_str().parse().unwrap()));
        node.add_dac_actor_relationship_as(&tx, "Record", &doc, "reader", READER)
            .await
            .unwrap();
        assert!(node
            .add_dac_actor_relationship_as(&tx, "Record", &doc, "missing", READER)
            .await
            .is_err());
        assert!(updates.try_recv().is_err());
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert_eq!(visible(&node).await, 1);
        let update = updates.try_recv().expect("committed grant publishes once");
        assert_eq!(update.as_update().unwrap().doc_id, doc);
        assert!(updates.try_recv().is_err());
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn queued_relationship_request_rechecks_expiration_at_its_turn() {
    use query::txn::{txn_job, TransactionRegistry};
    workers::block_on(async {
        let (node, doc) = fixture().await;
        let owner = actor(OWNER);
        let handle = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let registry = node.txn_registry.clone();
        let (entered_tx, entered_rx) = futures::channel::oneshot::channel();
        let (release_tx, release_rx) = futures::channel::oneshot::channel();
        let blocker_context = owner.clone();
        let blocker_handle = handle.clone();
        let blocker = workers::spawn(None, "hold-transaction-turn", move || async move {
            registry
                .run_as(
                    &blocker_context,
                    &blocker_handle,
                    None,
                    txn_job(move |_| {
                        Box::pin(async move {
                            entered_tx.send(()).unwrap();
                            release_rx.await.unwrap();
                        })
                    }),
                )
                .await
        })
        .unwrap();
        entered_rx.await.unwrap();

        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 2;
        let tx = owner
            .clone()
            .with_txn(Some(handle.as_str().parse().unwrap()))
            .with_authorization_expiration(Some(expires));
        let operation = node
            .acp_ops
            .add_dac_actor_relationship(&tx, OWNER, "Record", &doc, "reader", READER);
        futures::pin_mut!(operation);
        assert!(matches!(
            futures::poll!(&mut operation),
            std::task::Poll::Pending
        ));
        workers::delay(std::time::Duration::from_secs(3))
            .unwrap()
            .await
            .unwrap();
        release_tx.send(()).unwrap();
        blocker.await.unwrap().unwrap();
        assert!(operation.await.is_err());
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert_eq!(visible(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn transaction_admission_matches_executed_lean_model() {
    use serde_json::{json, Value};
    use std::io::Write;
    use std::process::{Command, Stdio};

    let cases = workers::block_on(async {
        let (node, doc) = fixture().await;
        let owner = actor(OWNER);
        let reader = actor(READER);
        let mut results = Vec::<Value>::new();
        for (transaction_actor, caller, writable, expired) in [
            (&owner, OWNER, true, false),
            (&reader, OWNER, true, false),
            (&owner, OWNER, false, false),
            (&reader, READER, true, false),
            (&owner, OWNER, true, true),
        ] {
            let handle = node
                .runner()
                .begin_txn_as(transaction_actor, !writable)
                .await
                .unwrap();
            let tx = actor(caller)
                .with_txn(Some(handle.as_str().parse().unwrap()))
                .with_authorization_expiration(expired.then_some(0));
            let accepted = node
                .add_dac_actor_relationship_as(&tx, "Record", &doc, "reader", READER)
                .await
                .is_ok();
            results.push(json!({
                "input": {
                    "sameOwner": transaction_actor.identity() == Some(caller),
                    "writable": writable, "nodeAllowed": true,
                    "documentAllowed": caller == OWNER, "boundProvider": true,
                    "authorizationFresh": !expired,
                    "transactionBackend": true,
                },
                "mayStage": accepted,
            }));
            node.runner()
                .discard_txn_as(transaction_actor, &handle)
                .await
                .unwrap();
        }
        let unbound = std::sync::Arc::new(acp::LocalDocumentACP::new(std::sync::Arc::new(
            acp::MemoryAcpStore::new(),
        )));
        let ops = super::acp_ops::DbAcpOps::new(
            node.database.clone(),
            node.txn_registry.clone(),
            unbound,
            node.event_bus.clone(),
        );
        let handle = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let tx = owner
            .clone()
            .with_txn(Some(handle.as_str().parse().unwrap()));
        let accepted = ops
            .add_dac_actor_relationship(&tx, OWNER, "Record", &doc, "reader", READER)
            .await
            .is_ok();
        results.push(
            json!({"input":{"sameOwner":true, "writable":true, "nodeAllowed":true,
            "documentAllowed":true, "boundProvider":false, "authorizationFresh":true,
            "transactionBackend":true},
            "mayStage":accepted}),
        );
        node.runner().discard_txn_as(&owner, &handle).await.unwrap();
        node.shutdown().await.unwrap();
        results
    })
    .unwrap();
    let inputs: Vec<_> = cases.iter().map(|case| case["input"].clone()).collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proofs/lean");
    let mut child = Command::new("lake")
        .current_dir(root)
        .args(["env", "lean", "--run", "TransactionalRelationship.lean"])
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
