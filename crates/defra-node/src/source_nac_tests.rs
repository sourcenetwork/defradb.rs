use defra_core::RequestContext;
use identity::Did;
use query::{txn::SourcePermission, TransactionHandle};

use super::Node;

const OWNER: &str = "did:key:owner";
const ACTOR: &str = "did:key:actor";
const OTHER: &str = "did:key:other";
const PERMISSION: acp::NodePermission = acp::NodePermission::CollectionPatch;

fn context(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

async fn begin(node: &Node, who: &RequestContext) -> (TransactionHandle, RequestContext) {
    let handle = node.runner().begin_txn_as(who, false).await.unwrap();
    let tx = who.clone().with_txn(Some(handle.as_str().parse().unwrap()));
    (handle, tx)
}

async fn change(node: &Node, tx: &RequestContext, source: &str, delete: bool) -> bool {
    node.change_nac_source_permission_as(tx, &edge(None, "source", source, delete))
        .await
        .unwrap()
}

fn edge(parent: Option<&str>, kind: &str, subject: &str, delete: bool) -> SourcePermission {
    SourcePermission {
        permission: PERMISSION.as_str().into(),
        parent_source: parent.map(str::to_owned),
        subject_kind: kind.into(),
        subject: subject.into(),
        delete,
    }
}

async fn allowed(node: &Node, did: &str) -> bool {
    node.nac_manager()
        .check_permission(&Did::new(did).unwrap(), PERMISSION)
        .await
        .unwrap()
}

#[test]
fn source_grants_are_transactional_and_independent() {
    workers::block_on(async {
        let node = Node::builder()
            .with_node_acp_owner(OWNER)
            .build()
            .await
            .unwrap();
        let owner = context(OWNER);
        let (handle, tx) = begin(&node, &owner).await;
        assert!(change(&node, &tx, "tools:one", false).await);
        assert!(!allowed(&node, ACTOR).await);
        node.runner().discard_txn_as(&owner, &handle).await.unwrap();
        assert!(!allowed(&node, ACTOR).await);
        let (handle, tx) = begin(&node, &owner).await;
        assert!(change(&node, &tx, "tools:one", false).await);
        assert!(change(&node, &tx, "tools:two", false).await);
        for item in [
            edge(Some("tools:one"), "source", "context:one", false),
            edge(Some("tools:two"), "source", "context:one", false),
            edge(Some("context:one"), "actor", ACTOR, false),
            edge(Some("context:one"), "actor", OTHER, false),
        ] {
            assert!(node
                .change_nac_source_permission_as(&tx, &item)
                .await
                .unwrap());
        }
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(allowed(&node, ACTOR).await);
        assert!(allowed(&node, OTHER).await);
        let (handle, tx) = begin(&node, &owner).await;
        assert!(change(&node, &tx, "tools:one", true).await);
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(allowed(&node, ACTOR).await);
        assert!(allowed(&node, OTHER).await);
        node.add_nac_actor_relationship_as(&owner, PERMISSION.as_str(), ACTOR)
            .await
            .unwrap();
        let (handle, tx) = begin(&node, &owner).await;
        assert!(change(&node, &tx, "tools:two", true).await);
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(allowed(&node, ACTOR).await);
        node.delete_nac_actor_relationship_as(&owner, PERMISSION.as_str(), ACTOR)
            .await
            .unwrap();
        assert!(!allowed(&node, ACTOR).await);
        assert!(!allowed(&node, OTHER).await);
        assert!(allowed(&node, OWNER).await);
        let (handle, tx) = begin(&node, &owner).await;
        assert!(change(&node, &tx, "tools:one", false).await);
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(allowed(&node, ACTOR).await);
        assert!(allowed(&node, OTHER).await);
        let (handle, tx) = begin(&node, &owner).await;
        assert!(change(&node, &tx, "tools:one", true).await);
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(!allowed(&node, ACTOR).await);
        assert!(!allowed(&node, OTHER).await);
        node.add_nac_actor_relationship_as(&owner, "admin", OTHER)
            .await
            .unwrap();
        assert!(allowed(&node, OTHER).await);
        node.delete_nac_actor_relationship_as(&owner, "admin", OTHER)
            .await
            .unwrap();
        assert!(!allowed(&node, OTHER).await);
        let outsider = context(OTHER);
        let (handle, tx) = begin(&node, &outsider).await;
        assert!(node
            .change_nac_source_permission_as(&tx, &edge(None, "source", "tools:one", false))
            .await
            .is_err());
        node.runner()
            .discard_txn_as(&outsider, &handle)
            .await
            .unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn revoked_source_invalidates_a_concurrent_claim() {
    workers::block_on(async {
        let node = Node::builder()
            .with_node_acp_owner(OWNER)
            .build()
            .await
            .unwrap();
        let owner = context(OWNER);
        node.add_collection_as(&owner, "type ClaimMarker { value: String }")
            .await
            .unwrap();
        let created = node.execute_request(
            query::QueryRequest::new("mutation { create_ClaimMarker(input:{value:\"before\"}) {_docID} }")
                .with_identity(Some(Did::new(OWNER).unwrap())),
        ).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc = created.data.unwrap()["add_ClaimMarker"][0]["_docID"].as_str().unwrap().to_owned();
        let (handle, tx) = begin(&node, &owner).await;
        assert!(change(&node, &tx, "tools:one", false).await);
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        let (claim, claim_tx) = begin(&node, &owner).await;
        assert!(!change(&node, &claim_tx, "tools:one", false).await);
        let request = query::QueryRequest::new(format!("mutation {{ update_ClaimMarker(docID:\"{doc}\", input:{{value:\"after\"}}) {{_docID}} }}"))
            .with_identity(Some(Did::new(OWNER).unwrap()))
            .with_context(owner.clone());
        let changed = node.execute_request_in_txn(request, &claim).await;
        assert!(!changed.has_errors(), "{:?}", changed.errors);
        let (revoke, revoke_tx) = begin(&node, &owner).await;
        assert!(change(&node, &revoke_tx, "tools:one", true).await);
        node.runner().commit_txn_as(&owner, &revoke).await.unwrap();
        assert!(node.runner().commit_txn_as(&owner, &claim).await.is_err());
        assert!(!allowed(&node, ACTOR).await);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn source_admission_matches_executed_lean_model() {
    use serde_json::json;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let observations = workers::block_on(async {
        let node = Node::builder().with_node_acp_owner(OWNER).build().await.unwrap();
        let mut observations = Vec::new();
        for (same_owner, writable, authorized, valid_source) in [
            (true, true, true, true), (false, true, true, true),
            (true, false, true, true), (true, true, false, true),
            (true, true, true, false),
        ] {
            let grantor = if authorized { OWNER } else { OTHER };
            let owner = context(grantor);
            let handle = node.runner().begin_txn_as(&owner, !writable).await.unwrap();
            let acting = if same_owner { grantor } else { OTHER };
            let tx = context(acting).with_txn(Some(handle.as_str().parse().unwrap()));
            let result = node.change_nac_source_permission_as(
                &tx, &edge(None, "source", if valid_source { "tools:one" } else { "" }, false),
            ).await;
            observations.push((json!({"sameOwner":same_owner,"writable":writable,"authorized":authorized,"boundStore":true,"fresh":true,"validSource":valid_source}), result.is_ok()));
            node.runner().discard_txn_as(&owner, &handle).await.unwrap();
        }
        node.shutdown().await.unwrap();
        observations
    }).unwrap();
    let inputs: Vec<_> = observations
        .iter()
        .map(|(input, _)| input.clone())
        .collect();
    let mut lean = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "SourceScopedNac.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(lean.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
    let output = lean.wait_with_output().unwrap();
    assert!(output.status.success());
    let model: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for (modeled, (_, actual)) in model.as_array().unwrap().iter().zip(observations) {
        assert_eq!(modeled["mayStage"].as_bool().unwrap(), actual, "{modeled}");
    }
}

#[test]
fn source_graph_survives_restart() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = Node::builder()
            .data_path(directory.path())
            .with_node_acp_owner(OWNER)
            .build()
            .await
            .unwrap();
        let owner = context(OWNER);
        let (handle, tx) = begin(&node, &owner).await;
        for item in [
            edge(None, "source", "tools:one", false),
            edge(Some("tools:one"), "source", "context:one", false),
            edge(Some("context:one"), "actor", ACTOR, false),
        ] {
            assert!(node
                .change_nac_source_permission_as(&tx, &item)
                .await
                .unwrap());
        }
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(allowed(&node, ACTOR).await);
        node.shutdown().await.unwrap();
        drop(node);
        let reopened = Node::builder()
            .data_path(directory.path())
            .with_node_acp_owner(OWNER)
            .build()
            .await
            .unwrap();
        assert!(allowed(&reopened, ACTOR).await);
        let (handle, tx) = begin(&reopened, &owner).await;
        assert!(change(&reopened, &tx, "tools:one", true).await);
        reopened
            .runner()
            .commit_txn_as(&owner, &handle)
            .await
            .unwrap();
        assert!(!allowed(&reopened, ACTOR).await);
        reopened.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn cyclic_source_graph_does_not_grant_an_unrelated_actor() {
    workers::block_on(async {
        let node = Node::builder()
            .with_node_acp_owner(OWNER)
            .build()
            .await
            .unwrap();
        let owner = context(OWNER);
        let (handle, tx) = begin(&node, &owner).await;
        for item in [
            edge(None, "source", "a", false),
            edge(Some("a"), "source", "b", false),
            edge(Some("b"), "source", "a", false),
        ] {
            assert!(node
                .change_nac_source_permission_as(&tx, &item)
                .await
                .unwrap());
        }
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(!allowed(&node, ACTOR).await);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn trusted_host_source_change_keeps_user_transaction_owner() {
    workers::block_on(async {
        let node = Node::builder()
            .enable_signing()
            .with_node_acp_owner(OWNER)
            .build()
            .await
            .unwrap();
        let node_did = node.node_identity_did().unwrap().to_owned();
        let owner = context(OWNER);
        let user = context(ACTOR);
        let (handle, tx) = begin(&node, &user).await;
        let root = edge(None, "source", "plugin:installed", false);
        assert!(node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .is_err());
        node.runner().discard_txn_as(&user, &handle).await.unwrap();
        node.add_nac_actor_relationship_as(&owner, "admin", &node_did)
            .await
            .unwrap();
        let (handle, tx) = begin(&node, &user).await;
        let wrong_owner = context(OTHER).with_txn(Some(handle.as_str().parse().unwrap()));
        assert!(node
            .change_host_nac_source_permission_in_txn(&wrong_owner, &root)
            .await
            .is_err());
        for item in [
            root.clone(),
            edge(Some("plugin:installed"), "actor", OTHER, false),
        ] {
            assert!(node
                .change_host_nac_source_permission_in_txn(&tx, &item)
                .await
                .unwrap());
        }
        assert!(!allowed(&node, OTHER).await);
        node.runner().discard_txn_as(&user, &handle).await.unwrap();
        assert!(!allowed(&node, OTHER).await);
        let (handle, tx) = begin(&node, &user).await;
        for item in [root, edge(Some("plugin:installed"), "actor", OTHER, false)] {
            assert!(node
                .change_host_nac_source_permission_in_txn(&tx, &item)
                .await
                .unwrap());
        }
        node.runner().commit_txn_as(&user, &handle).await.unwrap();
        assert!(allowed(&node, OTHER).await);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}
