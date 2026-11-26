//! Documents created and then updated at once, under concurrent load, on a
//! node whose document ACP is its own Vera node: the update's projection
//! observes the grant the create's projection staged. This is a load check
//! that such updates project without refusal; it does not force the grant to
//! be unsettled when the update observes it (the create's commit usually
//! settles it first), so it does not by itself decide the wait. The gate
//! tests in `db/tests/access/external_observation.rs` do
//! (`proofs/tla/SettleBeforeObserving.tla`).

use std::sync::Arc;

use acp::nac::NodePermission;
use identity::Did;
use regolith::sync::Atom;

use super::{create_user, created, registered_identity, starting_with, users};
use defra_node::{EmbeddedVeraActor, QueryRequest};

/// Grants `reader` to `reader` on a document it projects, unless the Vera
/// node already holds that grant.
struct GrantOnce {
    reader: Did,
    policy: Arc<Atom<Option<String>>>,
    rules: Vec<acp::projection::Rule>,
}

#[async_trait::async_trait]
impl db::DocumentMutationProjector for GrantOnce {
    fn projection_rules(&self) -> &[acp::projection::Rule] {
        &self.rules
    }

    async fn reconcile(
        &self,
        _context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
        let policy: Option<String> = Option::clone(&self.policy.load());
        let Some(policy) = policy.filter(|_| collection == "Users" && !doc_id.is_empty()) else {
            return Ok(());
        };
        if context
            .projected_actor_relationship(&policy, "users", doc_id, "reader", &self.reader)
            .await?
        {
            return Ok(());
        }
        context
            .change_projected_relationship(
                collection,
                doc_id,
                "reader",
                acp::Subject::entity(self.reader.clone()),
                false,
            )
            .await
            .map(|_| ())
    }
}

fn update_user(doc: &str, name: &str, actor: &Did) -> QueryRequest {
    QueryRequest::new(format!(
        "mutation {{ update_Users(docID: \"{doc}\", input: {{name: \"{name}\"}}) {{ _docID }} }}"
    ))
    .with_identity(Some(actor.clone()))
}

#[test]
fn creates_updated_at_once_under_load_project_without_refusal() {
    use crypto::Key as _;
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let admin = registered_identity();
        let reader = registered_identity();
        let policy_slot = Arc::new(Atom::new(None));
        let projector = Arc::new(GrantOnce {
            reader: reader.clone(),
            policy: Arc::clone(&policy_slot),
            rules: vec![acp::projection::Rule {
                collection: "Users".into(),
                resource: "users".into(),
                relation: "reader".into(),
                target_kind: acp::projection::TargetKind::Entity,
            }],
        });
        let node = starting_with(
            directory.path(),
            EmbeddedVeraActor::Secp256k1Key(vec![42; 32]),
            Vec::new(),
            None,
            |builder| {
                builder
                    .with_signing_key(defra_node::SigningKey::Ed25519(
                        crypto::generate_ed25519().unwrap().raw().to_vec(),
                    ))
                    .with_node_acp_owner(admin.to_string())
                    .with_document_projector(projector)
            },
        )
        .await
        .expect("node");
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let actor = Did::new(vera.self_did().expect("actor DID")).unwrap();
        let policy = vera
            .add_policy(actor.as_str(), super::POLICY, None)
            .await
            .expect("policy");
        drop(vera);
        node.add_collection(&defra_core::RequestContext::ambient(), &users(&policy))
            .await
            .expect("collection");
        let admin_context =
            defra_core::RequestContext::default().with_identity(Some(admin.to_string()));
        for permission in [
            NodePermission::DocumentUpdate,
            NodePermission::DocumentRead,
            NodePermission::CollectionGet,
            NodePermission::DacRelationAdd,
        ] {
            node.add_nac_actor_relationship_as(&admin_context, permission.as_str(), actor.as_str())
                .await
                .expect("node permission");
        }
        policy_slot.store(Some(policy.clone()));

        let lane = |index: usize| {
            let (node, actor) = (&node, &actor);
            async move {
                for round in 0..4 {
                    let doc = created(
                        node.execute_request(create_user(&format!("doc {index}-{round}"), actor))
                            .await,
                    );
                    let updated = node
                        .execute_request(update_user(&doc, "updated", actor))
                        .await;
                    assert!(!updated.has_errors(), "{:?}", updated.errors);
                }
            }
        };
        futures::future::join_all((0..8).map(lane)).await;
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
