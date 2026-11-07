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
        let denied = node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap_err();
        assert!(
            matches!(
                &denied,
                crate::Error::Transaction(query::TransactionError::Storage(
                    storage::Error::ProjectionPermissionDenied
                ))
            ),
            "Node authority must not substitute for the user: {denied:?}"
        );
        node.runner().discard_txn_as(&user, &handle).await.unwrap();
        let repair = context(&node_did);
        let (handle, tx) = begin(&node, &repair).await;
        assert!(node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap());
        node.runner()
            .discard_txn_as(&repair, &handle)
            .await
            .unwrap();
        node.add_nac_actor_relationship_as(&owner, "admin", ACTOR)
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

struct HostSourceObserver {
    fail: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl db::DocumentMutationProjector for HostSourceObserver {
    async fn reconcile(&self, _: &db::DocumentMutationContext<'_>) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        _: &db::DocumentMutationContext<'_>,
        _: &str,
        _: &str,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn source_changed(
        &self,
        _: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        if self.fail.load(std::sync::atomic::Ordering::Acquire) {
            return Err(storage::corekv::Error::Other(
                "host source projection refused".into(),
            ));
        }
        Ok(())
    }
}

#[test]
fn failed_host_source_projection_rolls_back_its_source_statement() {
    workers::block_on(async {
        let observer = std::sync::Arc::new(HostSourceObserver {
            fail: std::sync::atomic::AtomicBool::new(true),
        });
        let node = Node::builder()
            .enable_signing()
            .with_node_acp_enabled()
            .with_document_projector(observer.clone())
            .build()
            .await
            .unwrap();
        let user = context(ACTOR);
        let node_owner = context(node.node_identity_did().unwrap());
        node.add_nac_actor_relationship_as(&node_owner, "admin", ACTOR)
            .await
            .unwrap();
        let (handle, tx) = begin(&node, &user).await;
        let root = edge(None, "source", "plugin:failed", false);
        assert!(node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .is_err());
        node.runner().commit_txn_as(&user, &handle).await.unwrap();
        observer
            .fail
            .store(false, std::sync::atomic::Ordering::Release);
        let (handle, tx) = begin(&node, &user).await;
        node.change_host_nac_source_permission_in_txn(
            &tx,
            &edge(Some("plugin:failed"), "actor", OTHER, false),
        )
        .await
        .unwrap();
        node.runner().commit_txn_as(&user, &handle).await.unwrap();
        assert!(!allowed(&node, OTHER).await);
        let (handle, tx) = begin(&node, &user).await;
        node.change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap();
        node.runner().commit_txn_as(&user, &handle).await.unwrap();
        assert!(allowed(&node, OTHER).await);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn host_source_admission_matches_executed_lean_model() {
    use serde_json::json;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let observations = workers::block_on(async {
        let node = Node::builder()
            .enable_signing()
            .with_node_acp_owner(OWNER)
            .build()
            .await
            .unwrap();
        let node_did = Did::new(node.node_identity_did().unwrap()).unwrap();
        let owner = context(ACTOR);
        let admin = context(OWNER);
        let mut observations = Vec::new();
        for (node_is_granted, actor_is_granted, readonly, different_owner, expired, omit_transaction) in [
            (false, false, false, false, false, false),
            (true, false, false, false, false, false),
            (true, true, false, false, false, false),
            (true, true, false, true, false, false),
            (true, true, true, false, false, false),
            (true, true, false, false, true, false),
            (true, true, false, false, false, true),
        ] {
            if node_is_granted {
                node.add_nac_actor_relationship_as(&admin, "admin", node_did.as_str())
                    .await
                    .unwrap();
            }
            if actor_is_granted {
                node.add_nac_actor_relationship_as(&admin, "admin", ACTOR)
                    .await
                    .unwrap();
            }
            let handle = node.runner().begin_txn_as(&owner, readonly).await.unwrap();
            let actual_actor = if different_owner { OTHER } else { ACTOR };
            let mut tx = context(actual_actor)
                .with_authorization_expiration(expired.then_some(0));
            if !omit_transaction {
                tx = tx.with_txn(Some(handle.as_str().parse().unwrap()));
            }
            let inputs = json!({
                "trustedHost": true,
                "ownerMatches": tx.identity() == owner.identity(),
                "fresh": tx.authorization_valid_now(),
                "writable": !readonly,
                "configuredNode": node.node_identity_did() == Some(node_did.as_str()),
                // The edge carries PERMISSION, outside the admin class, and names a source.
                "actorAuthorized": node.nac_manager().is_admin(&Did::new(actual_actor).unwrap()).await.unwrap()
                    || (node.nac_manager().check_permission(&Did::new(actual_actor).unwrap(), acp::NodePermission::NacRelationAdd).await.unwrap()
                        && allowed(&node, actual_actor).await),
                "sameTransaction": tx.txn().is_some_and(|id| id.to_string() == handle.as_str()),
                "edgeChanged": true,
            });
            let accepted = node
                .change_host_nac_source_permission_in_txn(
                    &tx,
                    &edge(None, "source", "test:modeled", false),
                )
                .await
                .is_ok();
            observations.push((inputs, accepted));
            node.runner().discard_txn_as(&owner, &handle).await.unwrap();
        }
        node.shutdown().await.unwrap();
        let no_identity = Node::builder().with_node_acp_owner(OWNER).build().await.unwrap();
        let (handle, tx) = begin(&no_identity, &owner).await;
        let inputs = json!({
            "trustedHost": true,
            "ownerMatches": tx.identity() == owner.identity(),
            "fresh": tx.authorization_valid_now(),
            "writable": true,
            "configuredNode": no_identity.node_identity_did().is_some(),
            "actorAuthorized": false,
            "sameTransaction": tx.txn().is_some_and(|id| id.to_string() == handle.as_str()),
            "edgeChanged": true,
        });
        let accepted = no_identity
            .change_host_nac_source_permission_in_txn(
                &tx,
                &edge(None, "source", "test:modeled", false),
            )
            .await
            .is_ok();
        observations.push((inputs, accepted));
        no_identity.runner().discard_txn_as(&owner, &handle).await.unwrap();
        no_identity.shutdown().await.unwrap();
        observations
    })
    .unwrap();
    let inputs: Vec<_> = observations
        .iter()
        .map(|(input, _)| input.clone())
        .collect();
    let mut lean = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "HostSourceConformance.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(lean.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
    let output = lean.wait_with_output().unwrap();
    assert!(output.status.success());
    let model: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(model.as_array().unwrap().len(), observations.len());
    for (modeled, (_, actual)) in model.as_array().unwrap().iter().zip(observations) {
        assert_eq!(modeled["allowed"].as_bool().unwrap(), actual, "{modeled}");
    }
}

#[test]
fn host_source_noop_validates_the_edge_and_does_not_borrow_node_authority() {
    workers::block_on(async {
        let node = Node::builder()
            .enable_signing()
            .with_node_acp_enabled()
            .build()
            .await
            .unwrap();
        let owner = context(node.node_identity_did().unwrap());
        let user = context(ACTOR);
        node.add_collection_as(&owner, "type SourceNoopMarker { value: String }")
            .await
            .unwrap();
        let created = node.execute_request(query::QueryRequest::new(
            "mutation { create_SourceNoopMarker(input:{value:\"before\"}) { _docID } }",
        )).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let marker = created.data.unwrap()["add_SourceNoopMarker"][0]["_docID"]
            .as_str().unwrap().to_owned();
        let root = edge(None, "source", "plugin:stable", false);
        let (handle, tx) = begin(&node, &owner).await;
        assert!(node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap());
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();

        let (pending, tx) = begin(&node, &user).await;
        assert!(!node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap());
        node.runner().commit_txn_as(&user, &pending).await.unwrap();

        let (pending, tx) = begin(&node, &owner).await;
        assert!(!node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap());
        let updated = node.execute_request_in_txn(query::QueryRequest::new(format!(
            "mutation {{ update_SourceNoopMarker(docID:\"{marker}\",input:{{value:\"after\"}}) {{ _docID }} }}"
        )), &pending).await;
        assert!(!updated.has_errors(), "{:?}", updated.errors);
        let (remove, tx_owner) = begin(&node, &owner).await;
        assert!(node
            .change_host_nac_source_permission_in_txn(
                &tx_owner,
                &edge(None, "source", "plugin:stable", true)
            )
            .await
            .unwrap());
        node.runner().commit_txn_as(&owner, &remove).await.unwrap();
        assert!(
            node.runner().commit_txn_as(&owner, &pending).await.is_err(),
            "a changed observed edge must conflict"
        );
        let current = node.execute_request(query::QueryRequest::new(format!(
            "{{ SourceNoopMarker(docID:\"{marker}\") {{ value }} }}"
        ))).await;
        assert!(!current.has_errors(), "{:?}", current.errors);
        assert_eq!(current.data.unwrap()["SourceNoopMarker"][0]["value"], "before");

        let (handle, tx) = begin(&node, &user).await;
        assert!(!node
            .change_host_nac_source_permission_in_txn(
                &tx,
                &edge(None, "source", "plugin:stable", true)
            )
            .await
            .unwrap());
        let denied = node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap_err();
        assert!(
            matches!(
                &denied,
                crate::Error::Transaction(query::TransactionError::Storage(
                    storage::Error::ProjectionPermissionDenied
                ))
            ),
            "{denied:?}"
        );
        node.runner().commit_txn_as(&user, &handle).await.unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

struct NoopProjectionObserver {
    mode: std::sync::atomic::AtomicU8,
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl db::DocumentMutationProjector for NoopProjectionObserver {
    async fn reconcile(&self, _: &db::DocumentMutationContext<'_>) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        _: &db::DocumentMutationContext<'_>,
        _: &str,
        _: &str,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn source_changed(
        &self,
        context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        use acp::{Subject, ZanzibarStore};
        use std::sync::atomic::Ordering;
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mode = self.mode.load(Ordering::SeqCst);
        if mode == 0 {
            return Ok(());
        }
        let store = acp::zanzibar::store::TransactionZanzibarStore::new(
            context
                .view
                .sibling(storage::namespace::Namespace::Acpstore),
        );
        let object = format!("source.{}", hex::encode("plugin:stable"));
        let members = store
            .get_relation_subjects(
                acp::NODE_POLICY_ID,
                acp::NODE_RESOURCE_NAME,
                &object,
                PERMISSION.as_str(),
            )
            .await
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))?;
        if !members.contains(&Subject::entity(Did::new(OTHER).unwrap())) {
            context
                .change_source_permission(PERMISSION, Some("plugin:stable"), "actor", OTHER, false)
                .await?;
        }
        if mode == 2 {
            return Err(storage::corekv::Error::Other(
                "dependent projection refused".into(),
            ));
        }
        Ok(())
    }
}

#[test]
fn unchanged_host_root_reconciles_descendants_as_the_original_actor() {
    use std::sync::atomic::Ordering;
    workers::block_on(async {
        let observer = std::sync::Arc::new(NoopProjectionObserver {
            mode: std::sync::atomic::AtomicU8::new(0),
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let node = Node::builder()
            .enable_signing()
            .with_node_acp_enabled()
            .with_document_projector(observer.clone())
            .build()
            .await
            .unwrap();
        let owner = context(node.node_identity_did().unwrap());
        let user = context(ACTOR);
        let root = edge(None, "source", "plugin:stable", false);
        let (handle, tx) = begin(&node, &owner).await;
        assert!(node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap());
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(!allowed(&node, OTHER).await);

        observer.mode.store(1, Ordering::SeqCst);
        let (handle, tx) = begin(&node, &user).await;
        let denied = node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap_err();
        assert!(
            matches!(
                &denied,
                crate::Error::Transaction(query::TransactionError::Storage(
                    storage::Error::ProjectionPermissionDenied
                ))
            ),
            "{denied:?}"
        );
        node.runner().commit_txn_as(&user, &handle).await.unwrap();
        assert!(!allowed(&node, OTHER).await);

        observer.mode.store(2, Ordering::SeqCst);
        let (handle, tx) = begin(&node, &owner).await;
        assert!(node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .is_err());
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(
            !allowed(&node, OTHER).await,
            "failed projection rolled back its staged child"
        );

        observer.mode.store(1, Ordering::SeqCst);
        let (handle, tx) = begin(&node, &owner).await;
        assert!(!node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap());
        node.runner().commit_txn_as(&owner, &handle).await.unwrap();
        assert!(allowed(&node, OTHER).await);

        let (handle, tx) = begin(&node, &user).await;
        assert!(!node
            .change_host_nac_source_permission_in_txn(&tx, &root)
            .await
            .unwrap());
        node.runner().commit_txn_as(&user, &handle).await.unwrap();
        assert_eq!(observer.calls.load(Ordering::SeqCst), 5);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}
