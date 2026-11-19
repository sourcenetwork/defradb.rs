//! An exact relationship observation through the external gate
//! (`proofs/lean/ExactRelationshipObservation.lean`): authority comes first,
//! so a caller who may not manage the relation gets the one refusal whether
//! the document is registered, owes its registration, or does not exist; the
//! creator of an unsettled registration is told it is unsettled.

use std::sync::atomic::Ordering;

use acp::{ActorRelationshipObservation, DocumentACP, OwedOp};
use identity::Did;

use super::external_registration::{alice, bob, mallory, POLICY, RESOURCE};
use super::external_staging::{Change, Staging};

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
