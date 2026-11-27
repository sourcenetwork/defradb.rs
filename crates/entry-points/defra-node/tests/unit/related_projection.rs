//! `project_related` changes relationships on documents other than the one a
//! request wrote, with the same-document guard lifted. It still acts only
//! with the requester's own authority, and never on the owner relation
//! (`proofs/tla/ProjectionNoEscalation.tla`).

use std::sync::Arc;

use acp::nac::NodePermission;
use identity::Did;
use regolith::sync::Atom;

use crate::{Node, QueryRequest};

const RELATED: &str = "did:key:related";

/// What `project_related` changes on the next write: a document and a
/// relation, granted to [`RELATED`].
type Target = Option<(String, &'static str)>;

struct RelatedProjector {
    target: Arc<Atom<Target>>,
    rules: Vec<acp::projection::Rule>,
}

#[async_trait::async_trait]
impl db::DocumentMutationProjector for RelatedProjector {
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
        _context: &db::DocumentMutationContext<'_>,
        _collection: &str,
        _doc_id: &str,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project_related(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        _doc_id: &str,
    ) -> storage::corekv::Result<()> {
        let target: Target = Target::clone(&self.target.load());
        let Some((doc, relation)) = target else {
            return Ok(());
        };
        context
            .change_projected_relationship(
                collection,
                &doc,
                relation,
                acp::Subject::entity(Did::new(RELATED).unwrap()),
                false,
            )
            .await
            .map(|_| ())
    }
}

fn rule(relation: &str) -> acp::projection::Rule {
    acp::projection::Rule {
        collection: "Related".into(),
        resource: "records".into(),
        relation: relation.into(),
        target_kind: acp::projection::TargetKind::Entity,
    }
}

fn signing(key: &crypto::Ed25519PrivateKey) -> defra_core::signing::SigningConfig {
    use crypto::{Key as _, PrivateKey as _};
    let public = key.public_key();
    defra_core::signing::SigningConfig {
        key_type: defra_core::signing::SigningKeyType::Ed25519,
        private_key_bytes: key.raw_owned(),
        public_key_bytes: public.raw_owned(),
        public_key_hex: hex::encode(public.raw()),
        remote_signer: None,
        signing_authorization: None,
    }
}

async fn create(
    node: &Node,
    key: &crypto::Ed25519PrivateKey,
    did: &str,
    name: &str,
) -> crate::QueryResponse {
    node.execute_request(
        crate::EmbeddedSigner::from_config(signing(key))
            .unwrap()
            .attach(
                QueryRequest::new(format!(
                    "mutation {{ create_Related(input:{{name:\"{name}\"}}) {{ _docID }} }}"
                ))
                .with_identity(Some(Did::new(did).unwrap())),
            ),
    )
    .await
}

fn doc_id(result: crate::QueryResponse) -> String {
    assert!(!result.has_errors(), "{:?}", result.errors);
    result.data.unwrap()["add_Related"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn related_projection_grants_only_what_the_requester_could_grant_directly() {
    use identity::Identity as _;
    workers::block_on(async {
        let admin_key = crypto::generate_ed25519().unwrap();
        let admin = identity::RawIdentity::from_private_key(admin_key.clone())
            .unwrap()
            .did()
            .unwrap()
            .to_string();
        let node_key = crypto::generate_ed25519().unwrap();
        let target = Arc::new(Atom::new(None));
        let node = Node::builder()
            .with_signing_key(crate::SigningKey::Ed25519(signing(&node_key).private_key_bytes.clone()))
            .with_node_acp_owner(admin.clone())
            .with_document_projector(Arc::new(RelatedProjector {
                target: Arc::clone(&target),
                rules: vec![rule("reader"), rule("owner")],
            }))
            .build()
            .await
            .unwrap();
        let node_did = node.node_identity_did().unwrap().to_owned();
        for permission in [
            NodePermission::DocumentUpdate,
            NodePermission::DocumentRead,
            NodePermission::CollectionGet,
            NodePermission::DacRelationAdd,
        ] {
            node.with_identity(identity::Did::new(admin.clone()).unwrap()).add_nac_actor_relationship(permission.as_str(), &node_did)
                .await
                .unwrap();
        }
        let policy = node.with_identity(identity::Did::new(admin.clone()).unwrap()).add_dac_policy("name: Related projection\nresources:\n  - name: records\n    relations:\n      - name: reader\n        types: [actor]\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n", None).await.unwrap();
        node.with_identity(identity::Did::new(admin.clone()).unwrap()).add_collection(&format!("type Related @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}"),
        )
        .await
        .unwrap();
        let reads = |doc: String| {
            let node = &node;
            let policy = policy.clone();
            async move {
                node.document_acp()
                    .check_named_permission(&Did::new(RELATED).unwrap(), "read", &policy, "records", &doc)
                    .await
                    .unwrap()
            }
        };

        let own = doc_id(create(&node, &node_key, &node_did, "own").await);
        let foreign = doc_id(create(&node, &admin_key, &admin, "foreign").await);

        // The node owns `own`: writing another document grants there.
        target.store(Some((own.clone(), "reader")));
        doc_id(create(&node, &node_key, &node_did, "grants on own").await);
        assert!(reads(own.clone()).await);

        // The node does not manage `foreign`, and could not grant there
        // directly either: the projection is refused and its write undone.
        target.store(Some((foreign.clone(), "reader")));
        let refused = create(&node, &node_key, &node_did, "grants on foreign").await;
        assert!(
            format!("{:?}", refused.errors).contains("not document owner"),
            "a related grant borrowed authority: {:?}",
            refused.errors
        );
        assert!(!reads(foreign.clone()).await);
        assert!(node
            .with_identity(identity::Did::new(node_did.clone()).unwrap()).add_dac_actor_relationship("Related", &foreign, "reader", RELATED)
            .await
            .is_err());

        // The owner relation is refused even on the node's own document and
        // even when a rule names it.
        target.store(Some((own.clone(), "owner")));
        let owner = create(&node, &node_key, &node_did, "takes owner").await;
        assert!(
            format!("{:?}", owner.errors).contains("PERMISSION_DENIED"),
            "a related projection changed the owner: {:?}",
            owner.errors
        );
        assert_eq!(
            node.document_acp()
                .get_doc_owner(&policy, "records", &own)
                .await
                .unwrap()
                .map(|did| did.to_string()),
            Some(node_did.clone())
        );

        node.shutdown().await.unwrap();
    })
    .unwrap();
}
