//! A projection's relationship change for an external provider, staged with
//! an owner's authority prepared before the transaction
//! (`acp::outbox::prepare_owner_relationship`) and matched exactly inside it
//! (`TransactionOutbox::stage_prepared_owner`), as
//! `proofs/lean/ProjectionRelationship.lean`'s `mayStageExternal` requires:
//! only the owner prepares, and a preparation is taken for its own change of
//! its own provider and no other.

use std::sync::Arc;

use acp::outbox::{prepare_owner_relationship, PreparedOwnerRelationship};
use acp::{DocumentACP, OwedOp, StageRelationship, Staged, Subject};
use defra_core::RequestContext;
use identity::Did;
use query::txn::{txn_job, TransactionRegistry};
use query::{QueryExecutor as _, TransactionHandle};

use super::external_registration::{
    alice, bob, gate, mallory, RemoteAcp, MANAGER, POLICY, RESOURCE,
};
use super::external_staging::{carol, Change, Staging};

fn change<'a>(
    actor: &'a Did,
    doc: &'a str,
    subject: &Did,
    managers: &'a [String],
) -> StageRelationship<'a> {
    StageRelationship {
        txn: None,
        actor,
        subject: Subject::Entity(subject.clone()),
        policy_id: POLICY,
        resource: RESOURCE,
        object: doc,
        relation: "reader",
        managing_relations: managers,
        op: OwedOp::Add,
    }
}

impl Staging {
    /// Prepare `actor`'s change of `subject`'s reader on `doc` through
    /// `provider`, outside any transaction.
    async fn prepare(
        &self,
        provider: Arc<dyn DocumentACP>,
        actor: &Did,
        doc: &str,
        subject: &Did,
    ) -> acp::Result<Arc<PreparedOwnerRelationship>> {
        let managers = [MANAGER.to_string()];
        prepare_owner_relationship(provider, &change(actor, doc, subject, &managers))
            .await
            .map(Arc::new)
    }

    /// Stage `actor`'s change of `subject`'s reader on `doc` in `handle`'s
    /// transaction with `prepared`.
    async fn stage_prepared(
        &self,
        handle: &TransactionHandle,
        actor: Did,
        doc: String,
        subject: Did,
        prepared: Arc<PreparedOwnerRelationship>,
    ) -> acp::Result<Staged> {
        let gate: Arc<dyn DocumentACP> = self.gate.clone();
        let job = txn_job(move |turn| {
            Box::pin(async move {
                turn.begin_statement().unwrap();
                let result = {
                    let ctx = turn.context().unwrap();
                    let outbox = ctx
                        .doc_fetcher()
                        .transaction_outbox(gate)
                        .await
                        .unwrap()
                        .expect("an external provider keeps an outbox");
                    let managers = [MANAGER.to_string()];
                    outbox
                        .stage_prepared_owner(change(&actor, &doc, &subject, &managers), &prepared)
                        .await
                };
                turn.end_statement(true).unwrap();
                result
            })
        });
        self.node
            .registry
            .run_as(&RequestContext::default(), handle, None, job)
            .await
            .unwrap()
    }
}

#[test]
fn the_owner_stages_its_prepared_change_with_its_authority_checked() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        assert_eq!(staging.remote.owner(&doc), Some(alice()));

        let prepared = staging
            .prepare(staging.gate.clone(), &alice(), &doc, &bob())
            .await
            .unwrap();
        let handle = staging.begin().await;
        let staged = staging
            .stage_prepared(&handle, alice(), doc.clone(), bob(), prepared)
            .await
            .unwrap();
        staging.commit(&handle).await;

        assert!(staged.authority_checked);
        let queue = staging.queue().await;
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].actor, alice());
        assert_eq!(queue[0].tuple.object, doc);
    })
    .unwrap()
}

#[test]
fn only_the_owner_prepares_a_change() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        let mut manager = Change::new(alice(), &doc, OwedOp::Add, mallory());
        manager.relation = MANAGER;
        staging.on_provider(alice(), manager).await;
        assert!(staging.remote.manages(&mallory(), &doc));

        for actor in [mallory(), bob()] {
            assert!(
                staging
                    .prepare(staging.gate.clone(), &actor, &doc, &carol())
                    .await
                    .is_err(),
                "{actor} is not the owner"
            );
        }
    })
    .unwrap()
}

#[test]
fn a_preparation_is_taken_for_its_own_change_and_provider_only() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        let prepared = staging
            .prepare(staging.gate.clone(), &alice(), &doc, &bob())
            .await
            .unwrap();

        let handle = staging.begin().await;
        let other_subject = staging
            .stage_prepared(
                &handle,
                alice(),
                doc.clone(),
                carol(),
                Arc::clone(&prepared),
            )
            .await;
        let other_actor = staging
            .stage_prepared(
                &handle,
                mallory(),
                doc.clone(),
                bob(),
                Arc::clone(&prepared),
            )
            .await;
        let other = RemoteAcp::new();
        other
            .register_doc_object(&alice(), POLICY, RESOURCE, &doc)
            .await
            .unwrap();
        let elsewhere = staging
            .prepare(gate(&other, staging.store.clone()), &alice(), &doc, &bob())
            .await
            .unwrap();
        let other_provider = staging
            .stage_prepared(&handle, alice(), doc.clone(), bob(), elsewhere)
            .await;
        staging.node.runner.discard_txn(&handle).await.unwrap();

        assert!(other_subject.is_err());
        assert!(other_actor.is_err());
        assert!(other_provider.is_err(), "prepared through another provider");
        assert!(staging.queue().await.is_empty());
    })
    .unwrap()
}
