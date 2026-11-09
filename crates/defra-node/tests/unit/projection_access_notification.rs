use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crypto::{Key as _, PrivateKey as _};
use defra_core::signing::{SigningConfig, SigningKeyType};
use identity::Identity as _;
use storage::namespace::Namespace;

use crate::{EmbeddedSigner, Node, QueryRequest};

struct Target {
    policy: String,
    doc_id: String,
}

struct AccessProjector {
    target: Mutex<Option<Target>>,
    grant: AtomicBool,
}

#[async_trait::async_trait]
impl db::DocumentMutationProjector for AccessProjector {
    async fn reconcile(&self, _: &db::DocumentMutationContext<'_>) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        _: &str,
    ) -> storage::corekv::Result<()> {
        if collection != "AccessParent" {
            return Ok(());
        }
        let target = self
            .target
            .lock()
            .unwrap()
            .as_ref()
            .map(|target| (target.policy.clone(), target.doc_id.clone()));
        let Some((policy, doc_id)) = target else {
            return Ok(());
        };
        let changed = context
            .document_acp
            .change_node_admin_relationship(
                context.view.sibling(Namespace::Acpstore),
                context.node_did,
                acp::Subject::Wildcard,
                &policy,
                "children",
                &doc_id,
                "reader",
                !self.grant.load(Ordering::SeqCst),
            )
            .await
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))?;
        if changed {
            context
                .notify_access_changed("AccessChild", &doc_id)
                .await?;
        }
        Ok(())
    }
}

fn held_identity() -> (identity::RawIdentity, SigningConfig) {
    let private = crypto::generate_ed25519().unwrap();
    let public = private.public_key();
    let config = SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: private.raw_owned(),
        public_key_bytes: public.raw_owned(),
        public_key_hex: hex::encode(public.raw()),
        remote_signer: None,
        signing_authorization: None,
    };
    (
        identity::RawIdentity::from_private_key(private).unwrap(),
        config,
    )
}

fn exercise_projected_access_notifications() -> Vec<bool> {
    edge::testing::edge().block_on(async {
        let (user, user_config) = held_identity();
        let (observer, _) = held_identity();
        let user_did = user.did().unwrap().to_string();
        let observer_did = observer.did().unwrap();
        let signer = EmbeddedSigner::from_config(user_config).unwrap();
        let projector = Arc::new(AccessProjector { target: Mutex::new(None), grant: AtomicBool::new(true) });
        let node = Node::builder().enable_signing().with_node_acp_owner(user_did.clone())
            .with_document_projector(projector.clone()).build().await.unwrap();
        let node_did = node.node_identity_did().unwrap().to_owned();
        let admin = defra_core::RequestContext::default().with_identity(Some(user_did.clone()));
        for permission in [acp::nac::NodePermission::DacBypass, acp::nac::NodePermission::DacRelationAdd,
            acp::nac::NodePermission::DacRelationDelete] {
            node.add_nac_actor_relationship_as(&admin, permission.as_str(), &node_did).await.unwrap();
        }
        for permission in [acp::nac::NodePermission::DocumentRead, acp::nac::NodePermission::CollectionGet] {
            node.add_nac_actor_relationship_as(&admin, permission.as_str(), observer_did.as_str()).await.unwrap();
        }
        let policy = node.add_dac_policy_as(&admin, "name: Projected access\nresources:\n  - name: children\n    relations:\n      - name: reader\n        types: [actor]\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n").await.unwrap();
        node.add_collection(&format!("type AccessChild @policy(id: \"{policy}\", resource: \"children\") {{ name: String }}\ntype AccessParent {{ value: Boolean }}")).await.unwrap();
        let request = |query: &str| QueryRequest::new(query).with_identity(Some(user.did().unwrap()));
        let child = node.execute_request(signer.attach(request("mutation { create_AccessChild(input:{name:\"child\"}) {_docID} }"))).await;
        assert!(!child.has_errors(), "{:?}", child.errors);
        let child_id = child.data.unwrap()["add_AccessChild"][0]["_docID"].as_str().unwrap().to_owned();
        let parent = node.execute_request(signer.attach(request("mutation { create_AccessParent(input:{value:false}) {_docID} }"))).await;
        assert!(!parent.has_errors(), "{:?}", parent.errors);
        let parent_id = parent.data.unwrap()["add_AccessParent"][0]["_docID"].as_str().unwrap().to_owned();
        *projector.target.lock().unwrap() = Some(Target { policy, doc_id: child_id.clone() });
        let collection = node.get_collection("AccessChild").unwrap().unwrap();
        let mut changes = node.subscribe_collection_changes([collection.collection_id.clone()]);
        let observe = || QueryRequest::new("{ AccessChild {_docID} }").with_identity(Some(observer_did.clone()));
        let visible = |response: query::QueryResponse| response.data.unwrap()["AccessChild"].as_array().unwrap().len();
        assert_eq!(visible(node.execute_request(observe()).await), 0);
        let update = |value| format!("mutation {{ update_AccessParent(docID:\"{parent_id}\", input:{{value:{value}}}) {{_docID}} }}");
        let txn = node.begin_transaction(false).await.unwrap();
        let result = node.execute_request_in_txn(signer.attach(request(&update(true))), &txn).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        assert!(changes.try_recv().is_err());
        node.discard_transaction(&txn).await.unwrap();
        let aborted = changes.try_recv().is_ok();
        assert!(!aborted);
        assert_eq!(visible(node.execute_request(observe()).await), 0);
        let result = node.execute_request(signer.attach(request(&update(true)))).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let changed = changes.try_recv().expect("committed grant invalidation");
        assert_eq!(changed.changes.len(), 1);
        assert_eq!(changed.changes[0].doc_id, child_id);
        assert_eq!(visible(node.execute_request(observe()).await), 1);
        let result = node.execute_request(signer.attach(request(&update(false)))).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let unchanged = changes.try_recv().is_ok();
        assert!(!unchanged);
        projector.grant.store(false, Ordering::SeqCst);
        let result = node.execute_request(signer.attach(request(&update(true)))).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let changed = changes.try_recv().expect("committed revoke invalidation");
        assert_eq!(changed.changes.len(), 1);
        assert_eq!(changed.changes[0].doc_id, child_id);
        assert_eq!(visible(node.execute_request(observe()).await), 0);
        assert!(changes.try_recv().is_err());
        node.shutdown().await.unwrap();
        vec![aborted, true, unchanged, true]
    }).unwrap()
}

#[test]
fn projected_relationship_changes_notify_armed_observer_only_after_commit() {
    exercise_projected_access_notifications();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn projected_access_notifications_match_executed_lean_model() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let actual = exercise_projected_access_notifications();
    let input = serde_json::json!([
        {"before":false, "after":true, "committed":false},
        {"before":false, "after":true, "committed":true},
        {"before":true, "after":true, "committed":true},
        {"before":true, "after":false, "committed":true}
    ]);
    let mut child = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "ProjectionAccessNotification.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{input}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let modeled: Vec<bool> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, modeled);
}
