//! The drainer's turn: one pass over everything owed, with nothing it can
//! send.
//!
//! ```text
//! cargo bench -p benches --bench acp_owed_drain
//! ```
//!
//! The node's drainer runs a pass over the whole queue after every commit
//! that staged a change, and again after a back-off while the provider is
//! away. A change parked as refused stays in the queue, and so does every
//! later change of its tuple, so this is the work each turn repeats until
//! their actors move them. Each sample is one pass over a queue of `owed`
//! records: half refused, each holding back one staged change of its tuple.
//! `owed` reaches the node's bound on unsettled changes. The pass sends
//! nothing, so the provider is never asked.

use std::sync::Arc;

use acp::outbox::MAX_UNSETTLED_OWED_CHANGES;
use acp::{
    ExternalDocumentAcp, LocalDocumentACP, MemoryAcpStore, NewOwedChange, OwedAuthority, OwedOp,
    OwedScope, OwedState, OwedSubject, OwedTuple, StagedCount,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use datastore::BasicTxn;
use identity::Did;
use storage::RegolithStore;

mod common;

const POLICY: &str = "policy1";
const RESOURCE: &str = "file";
/// The actors changes are staged as, round robin, so none nears its own
/// bound on unsettled changes.
const ACTORS: usize = 16;

fn did(name: &str) -> Did {
    Did::new(format!("did:key:z6Mk{name}")).expect("a well-formed did")
}

fn change(tuple: usize, actor: usize, op: OwedOp) -> NewOwedChange {
    NewOwedChange {
        txn: None,
        tuple: OwedTuple {
            policy_id: POLICY.into(),
            resource: RESOURCE.into(),
            object: format!("doc{}", tuple % 64),
            relation: "reader".into(),
            subject: OwedSubject::Actor {
                did: did(&format!("s{tuple}")),
            },
        },
        op,
        actor: did(&format!("a{actor}")),
        authority: OwedAuthority::NodeKey,
        authority_checked: true,
    }
}

/// A gate whose queue holds `owed` changes: refused ones, each with a staged
/// change of its tuple behind it.
async fn queue_of(owed: usize) -> ExternalDocumentAcp<RegolithStore> {
    let store = Arc::new(RegolithStore::in_memory().expect("a store"));
    let provider = Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
    let gate = ExternalDocumentAcp::new(provider, store.clone());
    for index in 0..owed {
        let tuple = index / 2;
        let refused = index % 2 == 0;
        let op = if refused { OwedOp::Add } else { OwedOp::Delete };
        let txn = BasicTxn::new(store.as_ref(), 0, false)
            .await
            .expect("a transaction");
        let changes = gate.owed_changes(&txn.acpstore(), StagedCount::new());
        let seq = changes
            .stage(change(tuple, index % ACTORS, op))
            .await
            .expect("staged")
            .seq;
        if refused {
            let state = OwedState::Refused {
                reason: "the provider refused it".into(),
            };
            assert!(changes.mark(seq, state, 1, None).await.expect("parked"));
        }
        drop(changes);
        txn.commit().await.expect("committed");
    }
    gate
}

fn pass(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut group = c.benchmark_group("acp_owed_drain");
    group.sample_size(20);
    for owed in [0, 1_000, MAX_UNSETTLED_OWED_CHANGES as usize] {
        let gate = rt.block_on(queue_of(owed));
        group.bench_with_input(BenchmarkId::new("pass", owed), &owed, |b, &owed| {
            b.iter(|| {
                rt.block_on(async {
                    let settlement = gate
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

criterion_group!(benches, pass);
criterion_main!(benches);
