//! Owed ACP changes: staging one, settling one, deciding a denial, and how
//! often a transaction staging on a document conflicts with a settle there.
//!
//! ```text
//! cargo bench -p benches --bench acp_owed
//! ```
//!
//! A remote ACP provider is owed the relationship changes a transaction
//! stages; a pass settles them after the commit. `stage` and `settle` are one
//! transaction each, committed, on a document that already has changes owed;
//! changes name a fixed pool of subjects, so the store does not grow from one
//! sample to the next. `decide` is the deny overlay's read for one identity on such a document.
//! `conflicts` stages one change on a document while a change on the same
//! document settles, and reports the share of those commits that conflict:
//! the number #3070 is about. `pass` is the drainer's turn: one pass over
//! everything owed, over a queue of `owed` records it cannot send from, half
//! parked as refused and half held behind them, up to the node's bound.

use std::hint::black_box;
use std::sync::Arc;

use acp::outbox::MAX_UNSETTLED_OWED_CHANGES;
use acp::{
    ExternalDocumentAcp, Identity, LocalDocumentACP, MemoryAcpStore, NewOwedChange, OwedAuthority,
    OwedDenials, OwedOp, OwedScope, OwedState, OwedSubject, OwedTuple, StagedCount,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use datastore::BasicTxn;
use identity::Did;
use storage::RegolithStore;

mod common;

const POLICY: &str = "policy1";
const RESOURCE: &str = "file";
/// Changes owed on the document before each measurement.
const OWED: usize = 64;
/// Changes staged, or settled, before the other half of the work.
const CHUNK: u64 = 500;
/// The subjects changes name, reused round robin, so the counts a settle
/// keeps stay this many and every sample runs against the same store size.
const SUBJECTS: u64 = 64;

fn did(name: &str) -> Did {
    Did::new(format!("did:key:z6Mk{name}")).expect("a well-formed did")
}

fn change(object: &str, subject: &str, op: OwedOp) -> NewOwedChange {
    NewOwedChange {
        txn: None,
        tuple: OwedTuple {
            policy_id: POLICY.into(),
            resource: RESOURCE.into(),
            object: object.into(),
            relation: "reader".into(),
            subject: OwedSubject::Actor { did: did(subject) },
        },
        op,
        actor: did("owner"),
        authority: OwedAuthority::NodeKey,
        authority_checked: true,
    }
}

struct Outbox {
    store: Arc<RegolithStore>,
    gate: ExternalDocumentAcp<RegolithStore>,
}

impl Outbox {
    fn new() -> Self {
        let store = Arc::new(RegolithStore::in_memory().expect("a store"));
        let provider = Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
        let gate = ExternalDocumentAcp::new(provider, store.clone());
        Self { store, gate }
    }

    async fn begin(&self) -> (BasicTxn, acp::OwedChanges) {
        let txn = BasicTxn::new(self.store.as_ref(), 0, false)
            .await
            .expect("a transaction");
        let changes = self.gate.owed_changes(&txn.acpstore(), StagedCount::new());
        (txn, changes)
    }

    async fn stage(&self, new: NewOwedChange) -> u64 {
        let (txn, changes) = self.begin().await;
        let seq = changes.stage(new).await.expect("staged").seq;
        drop(changes);
        txn.commit().await.expect("committed");
        seq
    }

    async fn settle(&self, seq: u64) {
        let (txn, changes) = self.begin().await;
        assert!(changes.settle(seq).await.expect("settled"));
        drop(changes);
        txn.commit().await.expect("committed");
    }
}

fn operations(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let outbox = Outbox::new();
    rt.block_on(async {
        for index in 0..OWED {
            let op = if index % 2 == 0 {
                OwedOp::Add
            } else {
                OwedOp::Delete
            };
            outbox.stage(change("doc", &format!("s{index}"), op)).await;
        }
    });

    let mut group = c.benchmark_group("acp_owed");
    // Changes are staged and settled in chunks, so the queue never nears a
    // bound; only the half under test is timed.
    group.bench_function("stage", |b| {
        b.iter_custom(|iters| {
            rt.block_on(async {
                let mut timed = std::time::Duration::ZERO;
                let mut left = iters;
                while left > 0 {
                    let chunk = left.min(CHUNK);
                    let started = std::time::Instant::now();
                    let mut seqs = Vec::new();
                    for index in 0..chunk {
                        let subject = format!("n{}", index % SUBJECTS);
                        seqs.push(outbox.stage(change("doc", &subject, OwedOp::Delete)).await);
                    }
                    timed += started.elapsed();
                    for seq in seqs {
                        outbox.settle(seq).await;
                    }
                    left -= chunk;
                }
                timed
            })
        })
    });
    group.bench_function("settle", |b| {
        b.iter_custom(|iters| {
            rt.block_on(async {
                let mut timed = std::time::Duration::ZERO;
                let mut left = iters;
                while left > 0 {
                    let chunk = left.min(CHUNK);
                    let mut seqs = Vec::new();
                    for index in 0..chunk {
                        let subject = format!("m{}", index % SUBJECTS);
                        seqs.push(outbox.stage(change("doc", &subject, OwedOp::Add)).await);
                    }
                    let started = std::time::Instant::now();
                    for seq in seqs {
                        outbox.settle(seq).await;
                    }
                    timed += started.elapsed();
                    left -= chunk;
                }
                timed
            })
        })
    });
    let denied = Identity::Authenticated(did("s1"));
    let spared = Identity::Authenticated(did("nobody"));
    let provider = LocalDocumentACP::new(Arc::new(MemoryAcpStore::new()));
    group.bench_function("decide", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (txn, _) = outbox.begin().await;
                let denials = OwedDenials::new(&txn.acpstore());
                for identity in [&denied, &spared] {
                    black_box(
                        denials
                            .on(&provider, identity, POLICY, RESOURCE, "doc")
                            .await
                            .expect("a denial"),
                    );
                }
                txn.discard().expect("discarded");
            })
        })
    });
    group.finish();
}

/// The share of commits of a transaction staging on a document that
/// conflict with a settle committed there meanwhile. Printed, not timed.
fn conflicts(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let rounds = common::positive("ACP_OWED_ROUNDS", 200);
    let (conflicted, other_document) = rt.block_on(async {
        let outbox = Outbox::new();
        let mut conflicted = 0;
        let mut other_document = 0;
        for round in 0..rounds {
            for (object, count) in [("doc", &mut conflicted), ("other", &mut other_document)] {
                let settled = outbox
                    .stage(change(
                        "doc",
                        &format!("a{}", round as u64 % SUBJECTS),
                        OwedOp::Add,
                    ))
                    .await;
                let (txn, changes) = outbox.begin().await;
                changes
                    .stage(change(
                        object,
                        &format!("b{}", round as u64 % SUBJECTS),
                        OwedOp::Delete,
                    ))
                    .await
                    .expect("staged");
                drop(changes);
                outbox.settle(settled).await;
                if txn.commit().await.is_err() {
                    *count += 1;
                }
            }
        }
        (conflicted, other_document)
    });
    println!(
        "acp_owed/conflicts: same document {conflicted}/{rounds} ({:.1}%), other document {other_document}/{rounds}",
        100.0 * conflicted as f64 / rounds as f64
    );
    let _ = c;
}

/// A gate whose queue holds `owed` changes on 64 documents, staged by 16
/// actors: refused ones, each with a staged change of its tuple behind it.
async fn parked_queue(owed: usize) -> Outbox {
    let outbox = Outbox::new();
    for index in 0..owed {
        let tuple = index / 2;
        let refused = index % 2 == 0;
        let op = if refused { OwedOp::Add } else { OwedOp::Delete };
        let mut new = change(&format!("doc{}", tuple % 64), &format!("p{tuple}"), op);
        new.actor = did(&format!("actor{}", index % 16));
        let (txn, changes) = outbox.begin().await;
        let seq = changes.stage(new).await.expect("staged").seq;
        if refused {
            let state = OwedState::Refused {
                reason: "the provider refused it".into(),
            };
            assert!(changes.mark(seq, state, 1, None).await.expect("parked"));
        }
        drop(changes);
        txn.commit().await.expect("committed");
    }
    outbox
}

/// The drainer's turn: a pass over everything owed that sends nothing, as
/// each turn repeats while parked changes stay.
fn pass(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut group = c.benchmark_group("acp_owed");
    group.sample_size(20);
    for owed in [0, 1_000, MAX_UNSETTLED_OWED_CHANGES as usize] {
        let outbox = rt.block_on(parked_queue(owed));
        group.bench_with_input(BenchmarkId::new("pass", owed), &owed, |b, &owed| {
            b.iter(|| {
                rt.block_on(async {
                    let settlement = outbox
                        .gate
                        .settle_owed_changes_once(OwedScope::All)
                        .await
                        .expect("a pass");
                    assert_eq!(settlement.left as usize, owed / 2);
                })
            })
        });
    }
    group.finish();
}

criterion_group!(benches, operations, conflicts, pass);
criterion_main!(benches);
