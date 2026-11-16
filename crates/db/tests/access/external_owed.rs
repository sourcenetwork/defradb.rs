//! What a node owes an external ACP provider, as its callers act on it: the
//! report of owed registrations and changes with the reason of a failed
//! submission, the wait for one transaction's changes, a fresh credential for
//! one entry, and abandoning a refused change.

use std::sync::atomic::Ordering;

use acp::{
    DocumentACP, OwedAuthorityKind, OwedEntry, OwedFilter, OwedOp, OwedOutcome, OwedRef, OwedScope,
    OwedState, OwedWait, OwedWhat,
};
use identity::Did;
use query::QueryExecutor;

use super::external_registration::{alice, bob, mallory, MANAGER, POLICY, RESOURCE};
use super::external_staging::{Change, Staging};

fn registration(doc: &str) -> OwedRef {
    OwedRef::Registration {
        policy_id: POLICY.into(),
        resource: RESOURCE.into(),
        object: doc.into(),
    }
}

impl Staging {
    async fn owed_by(&self, actor: &Did) -> Vec<OwedEntry> {
        self.gate
            .owed_entries(OwedFilter::Actor(actor))
            .await
            .unwrap()
    }

    /// A document `owner` creates while the provider cannot be reached, so
    /// its registration stays owed.
    async fn create_unregistered(&self, owner: Did, name: &str) -> String {
        self.remote.down.store(true, Ordering::SeqCst);
        let doc = self.create(owner, name, None).await;
        self.remote.down.store(false, Ordering::SeqCst);
        doc
    }

    /// Mallory, a manager when the change is staged and no longer when it is
    /// submitted, makes `change`: the provider refuses it and it is parked.
    async fn refused_by_mallory(&self, doc: &str, op: OwedOp, subject: Did) -> u64 {
        let mut manager = Change::new(alice(), doc, OwedOp::Add, mallory());
        manager.relation = MANAGER;
        self.on_provider(alice(), manager.clone()).await;
        let staged = self
            .commit_change(Change::new(mallory(), doc, op, subject))
            .await;
        manager.op = OwedOp::Delete;
        self.on_provider(alice(), manager).await;
        let pass = self
            .gate
            .settle_owed_changes_once(OwedScope::All)
            .await
            .unwrap();
        assert_eq!(pass.refused, 1, "{pass:?}");
        staged.seq
    }
}

#[test]
fn a_registration_that_fails_after_its_commit_is_reported_with_its_reason() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create_unregistered(alice(), "secret").await;

        let owed = staging.owed_by(&alice()).await;
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].what, OwedWhat::Registration);
        assert_eq!(owed[0].actor, alice());
        assert_eq!(owed[0].object, doc);
        assert_eq!(owed[0].state, OwedState::Staged);
        assert!(
            owed[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("provider unreachable")),
            "{owed:?}"
        );
        assert_eq!(owed[0].authority, Some(OwedAuthorityKind::NodeKey));
        assert!(owed[0].sendable);
        assert_eq!(owed[0].reference(), registration(&doc));
        // Nobody else owes it.
        assert!(staging.owed_by(&bob()).await.is_empty());

        // A provider that says no is told apart from one that is away.
        staging
            .remote
            .refuse_registration
            .store(true, Ordering::SeqCst);
        assert_eq!(staging.gate.redrive_owed().await.unwrap().failed, 1);
        let owed = staging.owed_by(&alice()).await;
        assert!(
            matches!(&owed[0].state, OwedState::Refused { reason } if reason.contains("no credentials")),
            "{owed:?}"
        );
        assert_eq!(owed[0].attempts, 1);
        assert!(!owed[0].sendable);

        // It is tried again at the next start all the same, and reported no
        // more once the provider holds it.
        staging
            .remote
            .refuse_registration
            .store(false, Ordering::SeqCst);
        assert_eq!(staging.gate.redrive_owed().await.unwrap().settled, 1);
        assert_eq!(staging.remote.owner(&doc), Some(alice()));
        assert!(staging.owed_by(&alice()).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_registration_with_no_authority_waits_for_a_credential_and_is_not_sent() {
    workers::block_on(async {
        let staging = Staging::new().await;
        // The node holds no key for Alice and her request carries nothing.
        staging.remote.credentials.store(true, Ordering::SeqCst);
        let handle = staging.begin().await;
        let doc = staging.create(alice(), "secret", Some(&handle)).await;
        let _ = staging.node.runner.commit_txn(&handle).await;

        let parked = staging.owed_by(&alice()).await;
        assert_eq!(parked.len(), 1, "{parked:?}");
        assert_eq!(parked[0].state, OwedState::AwaitingAuthority);
        assert_eq!(parked[0].authority, None);
        assert!(!parked[0].sendable);
        assert!(
            parked[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("no key and no delegation")),
            "{parked:?}"
        );
        assert_eq!(staging.remote.registrations.load(Ordering::SeqCst), 0);

        // The retry at start asks for nothing it was not given.
        let redrive = staging.gate.redrive_owed().await.unwrap();
        assert_eq!((redrive.settled, redrive.failed), (0, 1));
        assert_eq!(staging.remote.registrations.load(Ordering::SeqCst), 0);
        assert_eq!(staging.remote.owner(&doc), None);

        // A credential that is not hers to give records nothing.
        assert!(matches!(
            staging
                .gate
                .reauthorize(&alice(), &registration(&doc))
                .await,
            Err(acp::Error::OwedAuthorityUnavailable { .. })
        ));
        assert!(matches!(
            staging.gate.reauthorize(&bob(), &registration(&doc)).await,
            Err(acp::Error::OwedEntryNotFound { .. })
        ));
        assert_eq!(staging.owed_by(&alice()).await, parked);

        staging.remote.carry_delegation(&alice(), "fresh");
        assert_eq!(
            staging
                .gate
                .reauthorize(&alice(), &registration(&doc))
                .await
                .unwrap(),
            OwedOutcome::Settled
        );
        assert_eq!(staging.remote.owner(&doc), Some(alice()));
        assert!(staging.owed_by(&alice()).await.is_empty());
        assert!(matches!(
            staging
                .gate
                .reauthorize(&alice(), &registration(&doc))
                .await,
            Err(acp::Error::OwedEntryNotFound { .. })
        ));
    })
    .unwrap()
}

#[test]
fn changes_behind_a_parked_registration_go_once_it_is_given_a_credential() {
    workers::block_on(async {
        let staging = Staging::new().await;
        staging.remote.credentials.store(true, Ordering::SeqCst);
        staging.remote.carry_delegation(&alice(), "short");
        let handle = staging.begin().await;
        let id: u64 = handle.as_str().parse().unwrap();
        let doc = staging.create(alice(), "shared", Some(&handle)).await;
        let mut grant = Change::new(alice(), &doc, OwedOp::Add, bob());
        grant.txn = Some(id);
        let staged = staging.stage(&handle, grant).await.unwrap();
        staging.remote.down.store(true, Ordering::SeqCst);
        let _ = staging.node.runner.commit_txn(&handle).await;
        staging.remote.down.store(false, Ordering::SeqCst);

        // Her delegation runs out, and with it the registration's authority
        // and the grant's.
        staging.remote.expire("short");
        staging.remote.forget_delegation(&alice());
        assert_eq!(
            staging
                .gate
                .await_transaction(id)
                .await
                .unwrap()
                .entries()
                .len(),
            2
        );
        let owed = staging
            .gate
            .owed_entries(OwedFilter::Transaction(id))
            .await
            .unwrap();
        assert_eq!(owed.len(), 2, "{owed:?}");
        assert_eq!(owed[0].what, OwedWhat::Registration);
        assert_eq!(owed[0].state, OwedState::AwaitingAuthority);
        assert_eq!(owed[1].state, OwedState::Staged);
        assert_eq!(owed[1].held_by, Some(registration(&doc)));
        assert!(!owed[1].sendable, "it waits behind a parked registration");
        assert!(matches!(
            staging.gate.await_transaction(id).await.unwrap(),
            OwedWait::Parked(entries) if entries.len() == 2
        ));
        assert!(staging.remote.take_writes().is_empty());

        // A fresh credential for the registration settles it. The grant
        // behind it is then sent with its own, which has run out too, and
        // is parked on its own account.
        staging.remote.carry_delegation(&alice(), "fresh");
        assert_eq!(
            staging
                .gate
                .reauthorize(&alice(), &registration(&doc))
                .await
                .unwrap(),
            OwedOutcome::Settled
        );
        assert_eq!(staging.remote.owner(&doc), Some(alice()));
        let owed = staging.owed_by(&alice()).await;
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].state, OwedState::AwaitingAuthority);
        assert_eq!(owed[0].held_by, None);
        assert!(!staging.remote.holds(&doc, &bob()));

        let change = OwedRef::Change { seq: staged.seq };
        assert!(matches!(
            staging.gate.reauthorize(&bob(), &change).await,
            Err(acp::Error::OwedEntryNotFound { .. })
        ));
        assert_eq!(
            staging.gate.reauthorize(&alice(), &change).await.unwrap(),
            OwedOutcome::Settled
        );
        assert!(staging.remote.holds(&doc, &bob()));
        assert_eq!(
            staging.gate.await_transaction(id).await.unwrap(),
            OwedWait::Settled
        );
    })
    .unwrap()
}

#[test]
fn a_wait_says_pending_while_the_provider_is_away_and_ends_when_a_record_changes() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        let handle = staging.begin().await;
        let id: u64 = handle.as_str().parse().unwrap();
        let mut grant = Change::new(alice(), &doc, OwedOp::Add, bob());
        grant.txn = Some(id);
        staging.stage(&handle, grant).await.unwrap();
        staging.commit(&handle).await;

        staging.remote.down.store(true, Ordering::SeqCst);
        assert!(matches!(
            staging.gate.await_transaction(id).await.unwrap(),
            OwedWait::Pending { entries, provider_unavailable: true } if entries.len() == 1
        ));
        staging.remote.down.store(false, Ordering::SeqCst);

        // The node cannot make the request now: the wait submits once, then
        // sleeps until another pass settles the change.
        staging.remote.unsendable.store(true, Ordering::SeqCst);
        let wait = staging.gate.await_transaction(id);
        let settle = async {
            while staging.queue().await[0].last_error.is_none() {
                futures_lite::future::yield_now().await;
            }
            staging.remote.unsendable.store(false, Ordering::SeqCst);
            staging
                .gate
                .settle_owed_changes_once(OwedScope::All)
                .await
                .unwrap()
        };
        let (waited, pass) = futures::join!(wait, settle);
        assert_eq!(pass.settled, 1);
        assert_eq!(waited.unwrap(), OwedWait::Settled);
        assert!(staging.remote.holds(&doc, &bob()));
        // Nothing is owed by a transaction that staged nothing.
        assert_eq!(
            staging.gate.await_transaction(id + 1000).await.unwrap(),
            OwedWait::Settled
        );
    })
    .unwrap()
}

#[test]
fn only_the_actor_that_staged_a_refused_delete_lifts_its_denial_by_abandoning_it() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(alice(), Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        assert!(staging.allowed(&bob(), &doc).await);

        let seq = staging
            .refused_by_mallory(&doc, OwedOp::Delete, bob())
            .await;
        let owed = staging.owed_by(&mallory()).await;
        assert!(
            matches!(&owed[0].state, OwedState::Refused { reason } if !reason.is_empty()),
            "{owed:?}"
        );
        // The provider holds the relationship and the refused delete denies
        // all the same.
        assert!(staging.remote.holds(&doc, &bob()));
        assert!(!staging.allowed(&bob(), &doc).await);

        // Neither the subject it names nor the document's owner drops it.
        for other in [bob(), alice()] {
            assert!(matches!(
                staging.gate.abandon_change(&other, seq).await,
                Err(acp::Error::OwedEntryNotFound { .. })
            ));
            assert!(!staging.allowed(&bob(), &doc).await);
        }

        staging.gate.abandon_change(&mallory(), seq).await.unwrap();
        assert!(staging.queue().await.is_empty());
        assert!(staging.owed_by(&mallory()).await.is_empty());
        // The provider, which refused the delete, decides again.
        assert!(staging.allowed(&bob(), &doc).await);
        assert!(staging.remote.holds(&doc, &bob()));
        assert!(matches!(
            staging.gate.abandon_change(&mallory(), seq).await,
            Err(acp::Error::OwedEntryNotFound { .. })
        ));
    })
    .unwrap()
}

#[test]
fn an_abandoned_change_lets_the_changes_behind_it_go_and_only_a_refused_one_is_abandoned() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(alice(), Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        staging.remote.take_writes();
        let refused = staging.refused_by_mallory(&doc, OwedOp::Add, bob()).await;

        // Alice's revocation of the same relationship waits behind it.
        let held = staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Delete, bob()))
            .await;
        let waiting = staging.owed_by(&alice()).await;
        assert_eq!(waiting[0].held_by, Some(OwedRef::Change { seq: refused }));
        assert!(!waiting[0].sendable);
        // A change that is only staged is not abandoned.
        assert!(matches!(
            staging.gate.abandon_change(&alice(), held.seq).await,
            Err(acp::Error::OwedChangeNotRefused { seq, state: "staged" }) if seq == held.seq
        ));

        staging
            .gate
            .abandon_change(&mallory(), refused)
            .await
            .unwrap();
        assert!(staging.queue().await.is_empty());
        assert_eq!(
            staging.remote.take_writes(),
            [(false, doc.clone(), bob().to_string())]
        );
        assert!(!staging.allowed(&bob(), &doc).await);
    })
    .unwrap()
}

#[test]
fn a_refused_change_is_submitted_again_under_a_fresh_credential() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        let seq = staging.refused_by_mallory(&doc, OwedOp::Add, bob()).await;
        let change = OwedRef::Change { seq };

        // Still no manager: the provider refuses it again.
        let outcome = staging.gate.reauthorize(&mallory(), &change).await.unwrap();
        assert!(
            matches!(&outcome, OwedOutcome::Owed(entry)
                if matches!(entry.state, OwedState::Refused { .. }) && entry.attempts == 1),
            "{outcome:?}"
        );

        let mut manager = Change::new(alice(), &doc, OwedOp::Add, mallory());
        manager.relation = MANAGER;
        staging.on_provider(alice(), manager).await;
        assert_eq!(
            staging.gate.reauthorize(&mallory(), &change).await.unwrap(),
            OwedOutcome::Settled
        );
        assert!(staging.remote.holds(&doc, &bob()));
    })
    .unwrap()
}
