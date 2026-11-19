//! Owed ACP changes: staging one, settling one, deciding a denial, and how
//! often a transaction staging on a document conflicts with a settle there.
//!
//! ```text
//! cargo bench -p benches --bench acp_owed
//! ```
//!
//! A remote ACP provider is owed the relationship changes a transaction
//! stages; a pass settles them after the commit. `stage` and `settle` are one
//! transaction each, committed, on a document that already has changes owed.
//! `decide` is the deny overlay's read for one identity on such a document.
//! `conflicts` stages one change on a document while a change on the same
//! document settles, and reports the share of those commits that conflict:
//! the number #3070 is about.

use std::hint::black_box;
use std::sync::Arc;

use acp::{
    ExternalDocumentAcp, Identity, LocalDocumentACP, MemoryAcpStore, NewOwedChange, OwedAuthority,
    OwedDenials, OwedOp, OwedSubject, OwedTuple, StagedCount,
};
use criterion::{criterion_group, criterion_main, Criterion};
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
    let mut round = 0u64;
    group.bench_function("stage", |b| {
        b.iter_custom(|iters| {
            rt.block_on(async {
                let mut timed = std::time::Duration::ZERO;
                let mut left = iters;
                while left > 0 {
                    let chunk = left.min(CHUNK);
                    round += 1;
                    let started = std::time::Instant::now();
                    let mut seqs = Vec::new();
                    for index in 0..chunk {
                        let subject = format!("n{round}-{index}");
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
                    round += 1;
                    let mut seqs = Vec::new();
                    for index in 0..chunk {
                        let subject = format!("m{round}-{index}");
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
                    .stage(change("doc", &format!("a{round}"), OwedOp::Add))
                    .await;
                let (txn, changes) = outbox.begin().await;
                changes
                    .stage(change(object, &format!("b{round}"), OwedOp::Delete))
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

criterion_group!(benches, operations, conflicts);
criterion_main!(benches);
