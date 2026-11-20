//! How long an exact observation waits for its owed change, and what it
//! leaves alone while it waits (`proofs/tla/SettleBeforeObserving.tla`): one
//! deadline ends every wait, authority still decides first when the deadline
//! comes, and the wait does not wake the drainer, so a provider failing for
//! a moment does not spend the change's tries.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use acp::{
    ActorRelationshipObservation, DocumentACP, OwedOp, OwedScope, OwedState, OwedSubject,
    OwedTuple, MAX_FAILED_SUBMISSIONS,
};
use datastore::BasicTxn;
use identity::Did;

use super::external_outbox::Gate;
use super::external_registration::{alice, bob, mallory, POLICY, RESOURCE};
use super::external_staging::{Change, Staging};

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

fn unsettled(result: &acp::Result<bool>) -> bool {
    matches!(result, Err(acp::Error::ProviderUnavailable(reason)) if reason == "relationship change is unsettled")
}

/// The longest an observation may take past its patience in these tests:
/// scheduling, not waiting.
const SLACK: Duration = Duration::from_millis(1500);

/// A stream of notifications of other owed records does not keep a wait
/// going past its deadline (`MC_SettleBeforeObserving_Red_WakeFirst`).
#[test]
fn notifications_of_other_records_do_not_extend_the_wait() {
    workers::block_on(async {
        let staging = staging_on().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        // Another thread notifies without pause, as commits of other owed
        // records would under load: a notification is nearly always pending
        // when the wait looks.
        let done = Arc::new(AtomicBool::new(false));
        let notifier = {
            let (gate, done) = (staging.gate.clone(), Arc::clone(&done));
            std::thread::spawn(move || {
                while !done.load(Ordering::SeqCst) {
                    gate.owed_changed();
                    std::thread::yield_now();
                }
            })
        };
        let started = std::time::Instant::now();
        let observed = observe(&staging, &alice(), &doc).await;
        done.store(true, Ordering::SeqCst);
        notifier.join().unwrap();
        assert!(
            started.elapsed() < acp::OBSERVATION_PATIENCE + SLACK,
            "waited {:?}",
            started.elapsed()
        );
        assert!(
            matches!(&observed, Err(acp::Error::ProviderUnavailable(reason)) if reason == "relationship change is unsettled"),
            "{observed:?}"
        );
    })
    .unwrap()
}

/// A settlement pass that clears the observed tuple and then holds the
/// settling lock past the deadline makes the observation refuse at its
/// deadline (`MC_SettleBeforeObserving_Red_LockUnbounded`).
#[test]
fn a_settlement_pass_holding_the_lock_past_the_deadline_refuses_the_observation() {
    workers::block_on(async {
        let staging = staging_on().await;
        let doc = staging.create(alice(), "shared", None).await;
        let other = staging.create(alice(), "other", None).await;
        staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        staging
            .commit_change(Change::new(alice(), &other, OwedOp::Add, bob()))
            .await;
        staging.remote.slow_submits.insert(
            other.clone(),
            acp::OBSERVATION_PATIENCE + Duration::from_secs(2),
        );

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
        let started = std::time::Instant::now();
        let observe = async {
            let observed = observe(&staging, &owner, &doc).await;
            (observed, started.elapsed())
        };
        let ((observed, took), ()) = futures::join!(observe, settle);
        assert!(staging.remote.holds(&doc, &bob()), "the tuple was settled");
        assert!(took < acp::OBSERVATION_PATIENCE + SLACK, "waited {took:?}");
        assert!(
            matches!(&observed, Err(acp::Error::ProviderUnavailable(reason)) if reason == "relationship change is unsettled"),
            "{observed:?}"
        );
    })
    .unwrap()
}

/// A caller who may not manage the relation is refused as such even when a
/// settlement pass holds the lock past the deadline: authority decides
/// before the observation gives up.
#[test]
fn a_caller_who_may_not_manage_is_refused_while_a_pass_holds_the_lock() {
    workers::block_on(async {
        let staging = staging_on().await;
        let doc = staging.create(alice(), "shared", None).await;
        let other = staging.create(alice(), "other", None).await;
        staging
            .commit_change(Change::new(alice(), &other, OwedOp::Add, bob()))
            .await;
        staging.remote.slow_submits.insert(
            other.clone(),
            acp::OBSERVATION_PATIENCE + Duration::from_secs(2),
        );

        let settle = async {
            staging
                .gate
                .settle_owed_changes_once(OwedScope::All)
                .await
                .unwrap();
        };
        let observe = async {
            workers::delay(Duration::from_millis(200))
                .unwrap()
                .await
                .unwrap();
            observe(&staging, &mallory(), &doc).await
        };
        let (observed, ()) = futures::join!(observe, settle);
        assert!(
            matches!(&observed, Err(acp::Error::NotManager { operation }) if operation == "observe relationship"),
            "{observed:?}"
        );
    })
    .unwrap()
}

/// The node's drainer as `defra-node` runs it: a pass, then a wait for a
/// commit, cut short by a back-off that doubles from a second while the
/// provider fails. Runs for `within` and returns how many passes it made.
async fn drain_for(gate: &Gate, within: Duration) -> u32 {
    let started = Instant::now();
    let mut backoff = Duration::from_secs(1);
    let mut passes = 0;
    loop {
        let settlement = gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        passes += 1;
        let left = within.saturating_sub(started.elapsed());
        if left.is_zero() {
            return passes;
        }
        let wait = if settlement.wants_another_pass() {
            let wait = backoff.min(left);
            backoff *= 2;
            wait
        } else {
            backoff = Duration::from_secs(1);
            left
        };
        let _ = workers::timeout(wait, gate.staged_changes_committed()).await;
    }
}

/// An observation waiting on a change the provider fails for a moment
/// leaves the drainer's back-off alone: the change keeps its tries, stays
/// owed, and settles once the provider takes it
/// (`MC_SettleBeforeObserving_Red_WakeDrainer`).
#[test]
fn an_observation_does_not_cut_the_drainers_backoff() {
    workers::block_on(async {
        let staging = staging_on().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        staging.remote.garble(Some("code 13"));

        let owner = alice();
        let (passes, observed) = futures::join!(
            drain_for(
                &staging.gate,
                acp::OBSERVATION_PATIENCE + Duration::from_millis(500)
            ),
            observe(&staging, &owner, &doc)
        );
        assert!(unsettled(&observed), "{observed:?}");
        let queue = staging.queue().await;
        assert_eq!(queue.len(), 1, "{queue:?}");
        assert_eq!(queue[0].state, OwedState::Staged, "after {passes} passes");
        assert!(
            queue[0].attempts < MAX_FAILED_SUBMISSIONS,
            "{passes} passes"
        );

        staging.remote.garble(None);
        staging
            .gate
            .settle_owed_changes_once(OwedScope::All)
            .await
            .unwrap();
        assert!(staging.remote.holds(&doc, &bob()));
    })
    .unwrap()
}

/// The tuple's head names its latest staged change and outlives its
/// settlement, so a projection's input keyed by it changes with every later
/// change on the tuple, settled or not
/// (`MC_SettleBeforeObserving_Red_NoHead`).
#[test]
fn the_tuple_head_moves_with_each_change_and_stays_once_it_settles() {
    workers::block_on(async {
        let staging = staging_on().await;
        let doc = staging.create(alice(), "shared", None).await;
        let tuple = OwedTuple {
            policy_id: POLICY.to_owned(),
            resource: RESOURCE.to_owned(),
            object: doc.clone(),
            relation: "reader".to_owned(),
            subject: OwedSubject::Actor { did: bob() },
        };
        let head = || async {
            let txn = BasicTxn::new(staging.store.as_ref(), 0, true)
                .await
                .unwrap();
            let head = acp::outbox::tuple_head(&txn.acpstore(), &tuple)
                .await
                .unwrap();
            txn.discard().unwrap();
            head
        };
        assert_eq!(head().await, None);

        staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;
        let granted = head().await.expect("the grant is the head");
        staging
            .gate
            .settle_owed_changes_once(OwedScope::All)
            .await
            .unwrap();
        assert!(staging.remote.holds(&doc, &bob()));
        assert_eq!(head().await, Some(granted), "settling keeps the head");

        staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Delete, bob()))
            .await;
        let revoked = head().await.expect("the revocation is the head");
        assert!(revoked > granted);
        staging
            .gate
            .settle_owed_changes_once(OwedScope::All)
            .await
            .unwrap();
        assert_eq!(head().await, Some(revoked));
    })
    .unwrap()
}
