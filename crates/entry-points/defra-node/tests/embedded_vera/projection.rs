//! A host's document projector on a node whose document ACP is its own Vera
//! node: a projected relationship on a registered document is decided at the
//! edge and staged in the transaction (`acp::outbox::prepare_relationship`,
//! `TransactionOutbox::stage_prepared`), for a manager of the relation as for
//! its owner, inside an explicit transaction as in an autocommit, and for the
//! actorless reconciliation that acts as the node.

use std::sync::Arc;

use acp::nac::NodePermission;
use acp::DocumentACP as _;
use crypto::Key as _;
use identity::Did;
use regolith::sync::Atom;

use super::{chain_reads, create_user, created, registered_identity, starting_with, users};
use defra_node::EmbeddedVeraActor;

const MANAGED_POLICY: &str = "\
name: managed
resources:
  - name: users
    permissions:
      - name: read
        expr: reader + manager
      - name: update
        expr: manager
      - name: delete
        expr: manager
    relations:
      - name: reader
        types:
          - actor
      - name: manager
        types:
          - actor
        manages:
          - reader
";

/// Grants `reader` to `reader` on whatever document the next write names
/// through `related`, and on every document in `reconcile` at reconciliation.
struct RelatedReader {
    reader: Did,
    related: Arc<Atom<Option<String>>>,
    /// The policy and document whose `reader` relationship of `reader` the
    /// next write observes, and what it saw.
    observe: Arc<Atom<Option<(String, String)>>>,
    observed: Arc<Atom<Option<bool>>>,
    /// The policy and document whose owner a source change asks for, and
    /// the owner it was given.
    source_owner: Arc<Atom<Option<(String, String)>>>,
    owner_seen: Arc<Atom<Option<Option<Did>>>>,
    reconcile: Arc<Atom<Vec<String>>>,
    rules: Vec<acp::projection::Rule>,
}

impl RelatedReader {
    async fn grant(
        &self,
        context: &db::DocumentMutationContext<'_>,
        doc: &str,
    ) -> storage::corekv::Result<()> {
        context
            .change_projected_relationship(
                "Users",
                doc,
                "reader",
                acp::Subject::entity(self.reader.clone()),
                false,
            )
            .await
            .map(|_| ())
    }
}

#[async_trait::async_trait]
impl db::DocumentMutationProjector for RelatedReader {
    fn projection_rules(&self) -> &[acp::projection::Rule] {
        &self.rules
    }

    async fn reconcile(
        &self,
        context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        let docs: Vec<String> = Vec::clone(&self.reconcile.load());
        for doc in docs {
            self.grant(context, &doc).await?;
        }
        Ok(())
    }

    async fn source_changed(
        &self,
        context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        let asked: Option<(String, String)> = Option::clone(&self.source_owner.load());
        if let Some((policy, doc)) = asked {
            let owner = context.projected_owner(&policy, "users", &doc).await?;
            self.owner_seen.store(Some(owner));
        }
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
        _collection: &str,
        _doc_id: &str,
    ) -> storage::corekv::Result<()> {
        let observe: Option<(String, String)> = Option::clone(&self.observe.load());
        if let Some((policy, doc)) = observe {
            let present = context
                .projected_actor_relationship(&policy, "users", &doc, "reader", &self.reader)
                .await?;
            self.observed.store(Some(present));
        }
        let related: Option<String> = Option::clone(&self.related.load());
        match related {
            Some(doc) => self.grant(context, &doc).await,
            None => Ok(()),
        }
    }
}

#[test]
fn a_projection_stages_through_vera_for_a_manager_in_a_transaction_and_for_the_node() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let admin = registered_identity();
        let reader = registered_identity();
        let related = Arc::new(Atom::new(None));
        let reconcile = Arc::new(Atom::new(Vec::new()));
        let observe = Arc::new(Atom::new(None));
        let observed = Arc::new(Atom::new(None));
        let source_owner = Arc::new(Atom::new(None));
        let owner_seen = Arc::new(Atom::new(None));
        let projector = Arc::new(RelatedReader {
            reader: reader.clone(),
            related: Arc::clone(&related),
            observe: Arc::clone(&observe),
            observed: Arc::clone(&observed),
            source_owner: Arc::clone(&source_owner),
            owner_seen: Arc::clone(&owner_seen),
            reconcile: Arc::clone(&reconcile),
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
            .add_policy(actor.as_str(), MANAGED_POLICY, None)
            .await
            .expect("policy");
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let manager = registered_identity();
        let node_did = node.node_identity_did().expect("node DID").to_owned();
        for who in [actor.as_str(), manager.as_str(), node_did.as_str()] {
            for permission in [
                NodePermission::DocumentUpdate,
                NodePermission::DocumentRead,
                NodePermission::CollectionGet,
                NodePermission::DacRelationAdd,
                NodePermission::DacRelationDelete,
            ] {
                node.with_identity(identity::Did::new(admin.to_string()).unwrap())
                    .add_nac_actor_relationship(permission.as_str(), who)
                    .await
                    .expect("node permission");
            }
        }

        let theirs = created(node.execute_request(create_user("theirs", &actor)).await);
        vera.add_actor_relationship(&actor, &manager, &policy, "users", &theirs, "manager", &[])
            .await
            .expect("manager");
        assert!(!chain_reads(&node, &reader, &policy, &theirs).await);

        // The manager writes another document in an explicit transaction;
        // the projection grants on `theirs` with the manager's authority.
        related.store(Some(theirs.clone()));
        let transaction = node.new_txn(false).await.unwrap();
        created(
            transaction
                .execute_request(create_user("by the manager", &manager))
                .await,
        );
        transaction.commit().await.expect("commit");
        related.store(None);
        assert!(chain_reads(&node, &reader, &policy, &theirs).await);

        // A stranger to `theirs` is refused the same projection.
        let stranger = registered_identity();
        for permission in [
            NodePermission::DocumentUpdate,
            NodePermission::DocumentRead,
            NodePermission::CollectionGet,
            NodePermission::DacRelationAdd,
        ] {
            node.with_identity(identity::Did::new(admin.to_string()).unwrap())
                .add_nac_actor_relationship(permission.as_str(), stranger.as_str())
                .await
                .expect("node permission");
        }
        let other = created(node.execute_request(create_user("other", &actor)).await);
        related.store(Some(other.clone()));
        let refused = node
            .execute_request(create_user("by a stranger", &stranger))
            .await;
        related.store(None);
        assert!(refused.has_errors(), "a stranger projected a grant");
        assert!(!chain_reads(&node, &reader, &policy, &other).await);

        // Reconciliation acts as the node, which owns `other` or manages it.
        if node_did != actor.as_str() {
            vera.add_actor_relationship(
                &actor,
                &Did::new(&node_did).unwrap(),
                &policy,
                "users",
                &other,
                "manager",
                &[],
            )
            .await
            .expect("node manager");
        }
        reconcile.store(vec![other.clone()]);
        node.database()
            .reconcile_document_projector()
            .await
            .expect("reconciliation");
        assert!(chain_reads(&node, &reader, &policy, &other).await);

        // A projection observes the relationship as the Vera node answered
        // it, unless a change of it is owed: a revocation staged earlier in
        // the same transaction makes that answer stale, and is refused.
        observe.store(Some((policy.clone(), theirs.clone())));
        created(node.execute_request(create_user("observes", &actor)).await);
        assert_eq!(Option::clone(&observed.load()), Some(true));
        let transaction = node.new_txn(false).await.unwrap();
        transaction
            .acting_for(identity::Did::new(actor.to_string()).unwrap())
            .change_dac_actor_relationship("Users", &theirs, "reader", reader.as_str(), true)
            .await
            .expect("staged revocation");
        let stale = transaction
            .execute_request(create_user("observes stale", &actor))
            .await;
        assert!(
            format!("{:?}", stale.errors).contains("relationship change is unsettled"),
            "{:?}",
            stale.errors
        );
        drop(transaction);
        observe.store(None);

        // A node source change replays the projector, which asks the Vera node
        // for an owner: the edge's statement is undone, the input answered at
        // the edge, and the edge run again with it.
        source_owner.store(Some((policy.clone(), theirs.clone())));
        let transaction = node.new_txn(false).await.unwrap();
        let changed = transaction
            .acting_for(identity::Did::new(admin.to_string()).unwrap())
            .change_nac_source_permission(&query::txn::SourcePermission {
                permission: NodePermission::CollectionPatch.as_str().into(),
                parent_source: None,
                subject_kind: "source".into(),
                subject: "tools:one".into(),
                delete: false,
            })
            .await
            .expect("a source change whose projection needs a prepared owner");
        assert!(changed);
        transaction.commit().await.expect("commit");
        source_owner.store(None);
        assert_eq!(Option::clone(&owner_seen.load()), Some(Some(actor.clone())));

        drop(vera);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
