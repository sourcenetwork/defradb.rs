//! An exact relationship observation through the external gate
//! (`proofs/lean/ExactRelationshipObservation.lean`): authority comes first,
//! so a caller who may not manage the relation gets the one refusal whether
//! the document is registered, owes its registration, or does not exist; the
//! creator of an unsettled registration is told it is unsettled.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use acp::{ActorRelationshipObservation, DocumentACP, OwedOp};
use identity::Did;

use super::external_registration::{alice, bob, mallory, ObserveHook, MANAGER, POLICY, RESOURCE};
use super::external_staging::{carol, Change, Staging};

/// A node on a provider that decides authority itself.
async fn staging_on() -> Staging {
    Staging::new().await
}

async fn observe(staging: &Staging, requester: &Did, doc: &str) -> acp::Result<bool> {
    staging
        .gate
        .observe_actor_relationship(ActorRelationshipObservation {
            requester,
            target: &bob(),
            policy: POLICY,
            resource: RESOURCE,
            document: doc,
            relation: "reader",
        })
        .await
}

fn refused(result: &acp::Result<bool>) -> bool {
    matches!(result, Err(acp::Error::NotManager { operation }) if operation == "observe relationship")
}

#[test]
fn an_unauthorized_caller_is_refused_alike_whatever_the_document_owes() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let registered = staging.create(alice(), "registered", None).await;
        staging.remote.down.store(true, Ordering::SeqCst);
        let pending = staging.create(alice(), "pending", None).await;
        staging.remote.down.store(false, Ordering::SeqCst);
        assert_eq!(
            staging.remote.owner(&pending),
            None,
            "its registration is owed"
        );

        for doc in [registered.as_str(), pending.as_str(), "bae-absent"] {
            let result = observe(&staging, &mallory(), doc).await;
            assert!(refused(&result), "{doc}: {result:?}");
        }
    })
    .unwrap()
}

#[test]
fn the_owner_observes_and_the_creator_of_an_owed_registration_waits() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let registered = staging.create(alice(), "registered", None).await;
        assert!(!observe(&staging, &alice(), &registered).await.unwrap());
        staging
            .on_provider(alice(), Change::new(alice(), &registered, OwedOp::Add, bob()))
            .await;
        assert!(observe(&staging, &alice(), &registered).await.unwrap());

        staging.remote.down.store(true, Ordering::SeqCst);
        let pending = staging.create(alice(), "pending", None).await;
        staging.remote.down.store(false, Ordering::SeqCst);
        assert!(matches!(
            observe(&staging, &alice(), &pending).await,
            Err(acp::Error::ProviderUnavailable(reason)) if reason == "document registration is unsettled"
        ));
    })
    .unwrap()
}

/// A settlement pass that would run between the provider's answer and the
/// gate's check of what is owed waits for the observation: the revocation
/// owed when the provider answered still refuses it, and is settled after
/// (`proofs/tla/ObservationSettlement.tla`).
#[test]
fn a_revocation_owed_when_the_provider_answered_is_not_settled_under_the_observation() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(alice(), Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Delete, bob()))
            .await;
        assert!(staging.remote.holds(&doc, &bob()), "the revocation is owed");

        let gate = staging.gate.clone();
        let settled = Arc::new(AtomicBool::new(false));
        let ran = Arc::clone(&settled);
        let hook: ObserveHook = Arc::new(move || {
            let (gate, ran) = (gate.clone(), Arc::clone(&ran));
            Box::pin(async move {
                let pass = gate.settle_owed_changes_once(acp::OwedScope::All);
                if let Ok(result) = workers::timeout(Duration::from_millis(300), pass).await {
                    result.unwrap();
                    ran.store(true, Ordering::SeqCst);
                }
            })
        });
        staging.remote.observing.insert((), hook);

        let observed = observe(&staging, &alice(), &doc).await;
        assert!(!settled.load(Ordering::SeqCst), "the pass waited");
        assert!(
            matches!(&observed, Err(acp::Error::ProviderUnavailable(reason)) if reason == "relationship change is unsettled"),
            "{observed:?}"
        );

        staging.remote.observing.remove(&());
        staging
            .gate
            .settle_owed_changes_once(acp::OwedScope::All)
            .await
            .unwrap();
        assert!(!observe(&staging, &alice(), &doc).await.unwrap());
    })
    .unwrap()
}

/// A grant committed a moment ago is still owed when the observation comes:
/// it waits for the settlement pass and answers from the state that holds
/// the grant (`proofs/tla/SettleBeforeObserving.tla`).
#[test]
fn an_observation_waits_for_a_change_about_to_settle() {
    workers::block_on(async {
        let staging = staging_on().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        assert!(!staging.remote.holds(&doc, &bob()), "the grant is owed");

        let settle = async {
            workers::delay(Duration::from_millis(200))
                .unwrap()
                .await
                .unwrap();
            staging
                .gate
                .settle_owed_changes_once(acp::OwedScope::All)
                .await
                .unwrap();
        };
        let owner = alice();
        let (observed, ()) = futures::join!(observe(&staging, &owner, &doc), settle);
        assert!(observed.unwrap(), "answered once the grant settled");
    })
    .unwrap()
}

/// A change that cannot settle refuses the observation: one nothing settles
/// once the patience is spent, a refused change at once.
#[test]
fn an_observation_refuses_a_change_that_cannot_settle() {
    workers::block_on(async {
        let staging = staging_on().await;
        let doc = staging.create(alice(), "shared", None).await;
        let unsettled = |result: &acp::Result<bool>| {
            matches!(result, Err(acp::Error::ProviderUnavailable(reason)) if reason == "relationship change is unsettled")
        };

        staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        // Nothing settles it here: the wait runs out its patience.
        let waited = observe(&staging, &alice(), &doc).await;
        assert!(unsettled(&waited), "{waited:?}");
        staging
            .gate
            .settle_owed_changes_once(acp::OwedScope::All)
            .await
            .unwrap();

        let mut manager = Change::new(alice(), &doc, OwedOp::Add, mallory());
        manager.relation = MANAGER;
        staging.on_provider(alice(), manager.clone()).await;
        staging
            .commit_change(Change::new(mallory(), &doc, OwedOp::Add, carol()))
            .await;
        manager.op = OwedOp::Delete;
        staging.on_provider(alice(), manager).await;
        staging
            .gate
            .settle_owed_changes_once(acp::OwedScope::All)
            .await
            .unwrap();
        let (owner, grantee) = (alice(), carol());
        let refused = staging
            .gate
            .observe_actor_relationship(ActorRelationshipObservation {
                requester: &owner,
                target: &grantee,
                policy: POLICY,
                resource: RESOURCE,
                document: &doc,
                relation: "reader",
            })
            .await;
        assert!(unsettled(&refused), "{refused:?}");
    })
    .unwrap()
}
