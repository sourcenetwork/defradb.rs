//! Owed changes sent through a provider that journals its requests: each
//! change is marked in flight before its request goes, a change found
//! marked after a crash is decided by the journal, a receipt or the note and
//! sent again only when none knows it, the changes one transaction staged
//! one after the other share a request, and a refused request is sent again
//! one change at a time (`proofs/tla/AcpOutbox.tla`, `Journaled = TRUE`).
//!
//! A crash is a new gate over the same store and the same provider: what
//! the drainer held in memory is gone, and what the store and the provider's
//! worker keep stays.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use acp::{
    DocumentACP, InFlight, OwedChange, OwedJournal, OwedOp, OwedScope, OwedSettlement, OwedState,
    PreparedRequest, RequestOutcome, Submission,
};
use identity::Did;
use regolith::sync::{HopscotchMap, SegQueue};
use storage::RegolithStore;

use super::external_outbox::Gate;
use super::external_registration::{alice, bob, gate, mallory, RemoteAcp, MANAGER};
use super::external_staging::{carol, Change, Staging};

type Map<K, V> = HopscotchMap<K, V, rapidhash::fast::RandomState>;

fn map<K, V>() -> Map<K, V>
where
    K: std::hash::Hash + Eq + Clone + 'static,
    V: Clone + 'static,
{
    HopscotchMap::with_hasher(rapidhash::fast::RandomState::default())
}

/// The request the worker's journal holds.
#[derive(Clone)]
struct Pending {
    digest: String,
    tx: String,
    changes: Vec<OwedChange>,
    /// What it came to, once it executed.
    landed: Option<bool>,
}

/// A provider's worker: one request journaled at a time, executed once,
/// its receipt kept by hash, and the outcome of a request resolved by anyone
/// but its own caller noted by digest.
struct Journal {
    remote: Weak<RemoteAcp>,
    prepared: Map<String, Vec<OwedChange>>,
    pending: Map<(), Pending>,
    receipts: Map<String, bool>,
    note: Map<String, bool>,
    /// How many changes each journaled request carried, in order.
    requests: SegQueue<usize>,
    next_tx: AtomicU64,
    /// The next journaling keeps the request and then the node crashes:
    /// the request's hash is never stored with its changes.
    crash_after_journal: AtomicBool,
    /// The next resolution executes the request and keeps its receipt, and
    /// then the node crashes before it stores what the request came to.
    crash_after_receipt: AtomicBool,
    /// The next resolution finds the request gone from the journal with no
    /// receipt or note of it: resolved by another, and since forgotten.
    forget_next: AtomicBool,
    /// The worker cannot build a request for the changes it is given.
    unsupported: AtomicBool,
}

impl Journal {
    fn on(remote: &Arc<RemoteAcp>) -> Arc<Self> {
        let journal = Arc::new(Self {
            remote: Arc::downgrade(remote),
            prepared: map(),
            pending: map(),
            receipts: map(),
            note: map(),
            requests: SegQueue::new(),
            next_tx: AtomicU64::new(1),
            crash_after_journal: AtomicBool::new(false),
            crash_after_receipt: AtomicBool::new(false),
            forget_next: AtomicBool::new(false),
            unsupported: AtomicBool::new(false),
        });
        let as_journal: Arc<dyn OwedJournal> = journal.clone();
        assert!(remote.journal.set(as_journal).is_ok());
        journal
    }

    fn remote(&self) -> Arc<RemoteAcp> {
        self.remote.upgrade().expect("the provider")
    }

    fn down(&self) -> bool {
        self.remote().down.load(Ordering::SeqCst)
    }

    /// The sizes of the requests journaled so far, taken out.
    fn take_requests(&self) -> Vec<usize> {
        std::iter::from_fn(|| self.requests.pop()).collect()
    }

    /// Execute `pending` once, all of its changes or none.
    async fn land(&self, pending: &mut Pending) -> bool {
        if let Some(applied) = pending.landed {
            return applied;
        }
        let remote = self.remote();
        let strict = remote.strict.load(Ordering::SeqCst);
        let applied = pending
            .changes
            .iter()
            .all(|change| !strict || remote.manages(&change.actor, &change.tuple.object));
        if applied {
            for change in &pending.changes {
                let answer = acp::outbox::submit_relationship(&*remote, change).await;
                assert_eq!(answer, Submission::Applied);
            }
        }
        pending.landed = Some(applied);
        applied
    }

    /// Resolve the journaled request: execute it, keep its receipt, note it
    /// when `note`, and let the journal go. What the provider does at start.
    async fn resolve(&self, note: bool) -> Option<(String, bool)> {
        let mut pending = self.pending.get(&())?;
        let applied = self.land(&mut pending).await;
        self.receipts.insert(pending.tx.clone(), applied);
        if note {
            self.note.insert(pending.digest.clone(), applied);
        }
        self.pending.remove(&());
        Some((pending.tx, applied))
    }
}

fn outcome(applied: bool, tx: &str) -> RequestOutcome {
    if applied {
        RequestOutcome::Applied
    } else {
        RequestOutcome::Rejected {
            reason: format!("submission {tx} was rejected"),
        }
    }
}

fn crashed() -> RequestOutcome {
    RequestOutcome::Unavailable {
        reason: "the node crashed".into(),
    }
}

#[async_trait::async_trait]
impl OwedJournal for Journal {
    async fn prepare(&self, changes: &[OwedChange]) -> Result<PreparedRequest, Submission> {
        if self.unsupported.load(Ordering::SeqCst) {
            return Err(Submission::Unsupported {
                reason: "the provider cannot take this change".into(),
            });
        }
        let seqs: Vec<String> = changes
            .iter()
            .map(|change| change.seq.to_string())
            .collect();
        let digest = format!("0xd{}", seqs.join("-"));
        self.prepared.insert(digest.clone(), changes.to_vec());
        Ok(PreparedRequest {
            digest: digest.clone(),
            bytes: digest.into_bytes(),
            carries: changes.len(),
            floor: None,
        })
    }

    async fn journal(&self, request: &PreparedRequest) -> Result<String, Submission> {
        if self.down() {
            return Err(Submission::Unavailable {
                reason: "provider unreachable".into(),
            });
        }
        if let Some(pending) = self.pending.get(&()) {
            if pending.digest == request.digest {
                return Ok(pending.tx);
            }
            self.resolve(true).await;
        }
        let tx = format!("0xt{}", self.next_tx.fetch_add(1, Ordering::SeqCst));
        let changes = self
            .prepared
            .get(&request.digest)
            .expect("a prepared request");
        self.requests.push(changes.len());
        self.pending.insert(
            (),
            Pending {
                digest: request.digest.clone(),
                tx: tx.clone(),
                changes,
                landed: None,
            },
        );
        if self.crash_after_journal.swap(false, Ordering::SeqCst) {
            return Err(Submission::Unavailable {
                reason: "the node crashed".into(),
            });
        }
        Ok(tx)
    }

    async fn outcome(&self, flight: &InFlight) -> RequestOutcome {
        if self.down() {
            return RequestOutcome::Unavailable {
                reason: "provider unreachable".into(),
            };
        }
        if self.forget_next.swap(false, Ordering::SeqCst) {
            self.pending.remove(&());
            return RequestOutcome::Unknown;
        }
        if let Some(pending) = self.pending.get(&()) {
            if flight.tx.as_deref() == Some(pending.tx.as_str()) || flight.digest == pending.digest
            {
                let (tx, applied) = self
                    .resolve(flight.tx.is_none())
                    .await
                    .expect("the journaled request");
                if self.crash_after_receipt.swap(false, Ordering::SeqCst) {
                    return crashed();
                }
                return outcome(applied, &tx);
            }
        }
        if let Some(tx) = &flight.tx {
            if let Some(applied) = self.receipts.get(tx) {
                return outcome(applied, tx);
            }
        }
        match self.note.get(&flight.digest) {
            Some(applied) => outcome(applied, &flight.digest),
            None => RequestOutcome::Unknown,
        }
    }

    fn refusal(&self, _change: &OwedChange, reason: String) -> Submission {
        Submission::Refused { reason }
    }

    fn applied(&self, _change: &OwedChange) {}
}

/// Every refusal the gate told of: the change's sequence, and whether it
/// stays parked.
#[derive(Default)]
struct Refusals(SegQueue<(u64, bool)>);

impl acp::OwedRefusalListener for Refusals {
    fn refused(&self, change: &OwedChange, _reason: &str, kept: bool) {
        self.0.push((change.seq, kept));
    }
}

/// The transaction the records of a test's changes name as staging them.
const STAGED_BY: u64 = 41;

fn add(actor: Did, doc: &str, subject: Did) -> Change {
    Change::new(actor, doc, OwedOp::Add, subject)
}

/// A node on a provider that journals its requests.
async fn journaled() -> (Staging, Arc<Journal>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let remote = RemoteAcp::new();
    let journal = Journal::on(&remote);
    (Staging::on(store, remote, true).await, journal)
}

impl Staging {
    /// The gate a node that restarts after a crash opens on the same store
    /// and provider.
    fn restarted(&self) -> Gate {
        gate(&self.remote, self.store.clone())
    }
}

async fn pass(gate: &Gate) -> OwedSettlement {
    gate.settle_owed_changes_once(OwedScope::All).await.unwrap()
}

fn settled(count: u64) -> OwedSettlement {
    OwedSettlement {
        settled: count,
        ..OwedSettlement::default()
    }
}

fn unavailable() -> OwedSettlement {
    OwedSettlement {
        provider_unavailable: true,
        ..OwedSettlement::default()
    }
}

#[test]
fn a_change_committed_before_a_crash_is_sent_once_after_the_restart() {
    workers::block_on(async {
        let (staging, journal) = journaled().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.commit_change(add(alice(), &doc, bob())).await;
        assert!(staging.remote.take_writes().is_empty());

        let restarted = staging.restarted();
        assert_eq!(pass(&restarted).await, settled(1));
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), bob().to_string())]
        );
        assert_eq!(journal.take_requests(), [1]);
        assert!(staging.queue().await.is_empty());
        assert_eq!(pass(&restarted).await, OwedSettlement::default());
        assert!(staging.remote.take_writes().is_empty());
    })
    .unwrap()
}

/// The request is journaled and the node crashes before the request's hash
/// is stored: the change names it by digest alone. The provider resolves the
/// journaled request at start and notes its outcome; the restarted drainer
/// finds the note by the digest and does not send the change again.
#[test]
fn a_change_marked_before_a_crash_is_decided_by_the_note_and_not_sent_again() {
    workers::block_on(async {
        let (staging, journal) = journaled().await;
        let doc = staging.create(alice(), "shared", None).await;
        let staged = staging.commit_change(add(alice(), &doc, bob())).await;

        journal.crash_after_journal.store(true, Ordering::SeqCst);
        assert_eq!(staging.pass_once().await, unavailable());
        let marked = staging.queue().await.remove(0);
        assert_eq!(marked.seq, staged.seq);
        let flight = marked.in_flight.expect("marked before it was journaled");
        assert_eq!(flight.tx, None, "the crash came before the hash was stored");
        assert!(staging.remote.take_writes().is_empty());

        // The provider at start: the journaled request executes, its outcome
        // is noted, and the journal lets it go.
        assert!(journal.resolve(true).await.is_some());
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), bob().to_string())]
        );

        let restarted = staging.restarted();
        assert_eq!(pass(&restarted).await, settled(1));
        assert!(staging.remote.take_writes().is_empty(), "sent once");
        assert_eq!(journal.take_requests(), [1]);
        assert!(staging.queue().await.is_empty());
    })
    .unwrap()
}

/// As above, with the journaled request still there when the drainer
/// restarts: it takes the request up by its digest and resolves it itself.
#[test]
fn a_change_marked_before_a_crash_is_decided_by_the_journal_and_not_sent_again() {
    workers::block_on(async {
        let (staging, journal) = journaled().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.commit_change(add(alice(), &doc, bob())).await;

        journal.crash_after_journal.store(true, Ordering::SeqCst);
        assert_eq!(staging.pass_once().await, unavailable());

        let restarted = staging.restarted();
        assert_eq!(pass(&restarted).await, settled(1));
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), bob().to_string())]
        );
        assert_eq!(journal.take_requests(), [1], "nothing journaled again");
        assert!(staging.queue().await.is_empty());
    })
    .unwrap()
}

/// The request executes and its receipt is kept, and the node crashes
/// before it stores what the request came to: the restarted drainer finds
/// the receipt by the stored hash.
#[test]
fn a_change_whose_receipt_was_kept_before_a_crash_is_settled_by_its_hash() {
    workers::block_on(async {
        let (staging, journal) = journaled().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.commit_change(add(alice(), &doc, bob())).await;

        journal.crash_after_receipt.store(true, Ordering::SeqCst);
        assert_eq!(staging.pass_once().await, unavailable());
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), bob().to_string())]
        );
        let flight = staging.queue().await.remove(0).in_flight.unwrap();
        assert!(flight.tx.is_some(), "the hash was stored before the send");

        let restarted = staging.restarted();
        assert_eq!(pass(&restarted).await, settled(1));
        assert!(staging.remote.take_writes().is_empty(), "sent once");
        assert!(staging.queue().await.is_empty());
        assert!(staging.allowed(&bob(), &doc).await);
    })
    .unwrap()
}

/// While the provider is away the change stays marked and nothing is
/// journaled; once it is back the drainer finds no trace of the request and
/// sends the change, once.
#[test]
fn an_unavailable_provider_leaves_the_change_marked_until_it_comes_back() {
    workers::block_on(async {
        let (staging, journal) = journaled().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.commit_change(add(alice(), &doc, bob())).await;

        staging.remote.down.store(true, Ordering::SeqCst);
        assert_eq!(staging.pass_once().await, unavailable());
        assert_eq!(staging.pass_once().await, unavailable());
        assert!(staging.queue().await[0].in_flight.is_some());
        assert!(journal.take_requests().is_empty());

        staging.remote.down.store(false, Ordering::SeqCst);
        assert_eq!(staging.pass_once().await, settled(1));
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), bob().to_string())]
        );
        assert_eq!(journal.take_requests(), [1]);
        assert!(staging.queue().await.is_empty());
    })
    .unwrap()
}

/// A checked change the provider refuses is parked and told of; a later
/// change of its tuple waits behind it, and the changes of other tuples go.
#[test]
fn a_refused_change_is_parked_and_told_of_and_holds_back_no_other_tuple() {
    workers::block_on(async {
        let (staging, journal) = journaled().await;
        let refusals = Arc::new(Refusals::default());
        staging.gate.watch_owed_refusals(refusals.clone());
        let doc = staging.create(alice(), "shared", None).await;
        let mut manager = add(alice(), &doc, mallory());
        manager.relation = MANAGER;
        staging.on_provider(alice(), manager.clone()).await;

        // Mallory manages when the change is staged, and no longer when it
        // is sent.
        let refused = staging.commit_change(add(mallory(), &doc, bob())).await;
        assert!(refused.authority_checked);
        manager.op = OwedOp::Delete;
        staging.on_provider(alice(), manager).await;
        let held = staging
            .commit_change(Change::new(alice(), &doc, OwedOp::Delete, bob()))
            .await;
        staging.commit_change(add(alice(), &doc, carol())).await;
        staging.remote.take_writes();

        assert_eq!(
            staging.pass_once().await,
            OwedSettlement {
                settled: 1,
                refused: 1,
                left: 1,
                ..OwedSettlement::default()
            }
        );
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), carol().to_string())]
        );
        assert_eq!(journal.take_requests(), [1, 1]);
        assert_eq!(refusals.0.pop(), Some((refused.seq, true)));
        assert_eq!(refusals.0.pop(), None);
        let queue = staging.queue().await;
        assert!(matches!(queue[0].state, OwedState::Refused { .. }));
        assert_eq!(queue[0].in_flight, None);
        assert_eq!(queue[1].seq, held.seq);
        assert_eq!(queue[1].state, OwedState::Staged);
    })
    .unwrap()
}

/// The changes one transaction staged one after the other go in one
/// request, which the provider applies whole.
#[test]
fn the_changes_one_transaction_staged_go_in_one_request() {
    workers::block_on(async {
        let (staging, journal) = journaled().await;
        let doc = staging.create(alice(), "shared", None).await;
        let handle = staging.begin().await;
        let mut first = add(alice(), &doc, bob());
        first.txn = Some(STAGED_BY);
        let mut second = add(alice(), &doc, carol());
        second.txn = Some(STAGED_BY);
        let mut revoked = Change::new(alice(), &doc, OwedOp::Delete, bob());
        revoked.txn = Some(STAGED_BY);
        for change in [first, second, revoked] {
            staging.stage(&handle, change).await.unwrap();
        }
        staging.commit(&handle).await;

        assert_eq!(staging.pass_once().await, settled(3));
        assert_eq!(journal.take_requests(), [3]);
        assert_eq!(
            staging.remote.take_writes(),
            [
                (true, doc.clone(), bob().to_string()),
                (true, doc.clone(), carol().to_string()),
                (false, doc.clone(), bob().to_string()),
            ]
        );
        assert!(!staging.remote.holds(&doc, &bob()));
        assert!(staging.queue().await.is_empty());
    })
    .unwrap()
}

/// A request the provider refuses applied nothing: its changes go again
/// one at a time, so the owner's change lands and only the change the
/// provider refuses on its own is parked.
#[test]
fn a_refused_request_is_sent_again_one_change_at_a_time() {
    workers::block_on(async {
        let (staging, journal) = journaled().await;
        let doc = staging.create(alice(), "shared", None).await;
        let mut manager = add(alice(), &doc, mallory());
        manager.relation = MANAGER;
        staging.on_provider(alice(), manager.clone()).await;

        let handle = staging.begin().await;
        let mut owner = add(alice(), &doc, carol());
        owner.txn = Some(STAGED_BY);
        let mut managed = add(mallory(), &doc, bob());
        managed.txn = Some(STAGED_BY);
        staging.stage(&handle, owner).await.unwrap();
        let refused = staging.stage(&handle, managed).await.unwrap();
        staging.commit(&handle).await;
        manager.op = OwedOp::Delete;
        staging.on_provider(alice(), manager).await;
        staging.remote.take_writes();

        assert_eq!(
            staging.pass_once().await,
            OwedSettlement {
                settled: 1,
                refused: 1,
                ..OwedSettlement::default()
            }
        );
        assert_eq!(journal.take_requests(), [2, 1, 1]);
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), carol().to_string())]
        );
        let queue = staging.queue().await;
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].seq, refused.seq);
        assert!(matches!(queue[0].state, OwedState::Refused { .. }));
        assert!(queue[0].alone);
    })
    .unwrap()
}

/// A request that leaves the journal with nobody knowing its outcome had
/// no effect the node can tell of: its change is to be sent again, and the
/// pass says so, so the drainer runs another without waiting for a commit.
/// The next pass sends it.
#[test]
fn a_change_whose_request_nobody_knows_is_tried_again_without_a_commit() {
    workers::block_on(async {
        let (staging, journal) = journaled().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.commit_change(add(alice(), &doc, bob())).await;

        journal.forget_next.store(true, Ordering::SeqCst);
        let settlement = staging.pass_once().await;
        assert_eq!(
            settlement,
            OwedSettlement {
                retry: 1,
                ..OwedSettlement::default()
            }
        );
        assert!(settlement.wants_another_pass());
        let queue = staging.queue().await;
        assert_eq!(queue[0].state, OwedState::Staged);
        assert_eq!(queue[0].in_flight, None);
        assert!(staging.remote.take_writes().is_empty());

        let settlement = staging.pass_once().await;
        assert_eq!(settlement, settled(1));
        assert!(!settlement.wants_another_pass());
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), bob().to_string())]
        );
        assert!(staging.queue().await.is_empty());
    })
    .unwrap()
}

/// A parked change is no reason for another pass: only its actor moves it.
#[test]
fn a_parked_change_asks_for_no_other_pass() {
    let parked = OwedSettlement {
        refused: 1,
        awaiting_authority: 1,
        discarded: 1,
        left: 1,
        ..OwedSettlement::default()
    };
    assert!(!parked.wants_another_pass());
    for retried in [
        OwedSettlement {
            failed: 1,
            ..parked
        },
        OwedSettlement {
            retry: 1,
            ..parked
        },
        OwedSettlement {
            provider_unavailable: true,
            ..parked
        },
    ] {
        assert!(retried.wants_another_pass(), "{retried:?}");
    }
}

impl Staging {
    async fn pass_once(&self) -> OwedSettlement {
        pass(&self.gate).await
    }
}
