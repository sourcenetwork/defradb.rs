use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

use super::{PendingDagFetchSlot, PendingDagFetchSlots};
use crate::network::test_network;
use crate::sync::coordinator::SyncShutdownHandle;

fn root(name: &[u8]) -> Cid {
    Cid::new_v1(0x55, Code::Sha2_256.digest(name))
}

fn slot(root_cid: Cid, state: &'static str) -> PendingDagFetchSlot {
    PendingDagFetchSlot {
        root_cid: root_cid.to_string(),
        state,
    }
}

/// Each slot is reported under the state that holds it, and leaves the
/// gauge when its owner releases it.
#[nuclei::test(threads = 0)]
async fn slots_report_the_state_holding_each_root() {
    let shutdown = SyncShutdownHandle::new(2, test_network());
    let (scheduled, running) = (root(b"scheduled"), root(b"running"));
    assert_eq!(
        shutdown.pending_dag_fetch_slots(),
        PendingDagFetchSlots {
            limit: 2,
            ..Default::default()
        }
    );

    assert!(shutdown.reserve_pending_dag_fetch(scheduled));
    assert!(shutdown
        .spawn_pending_dag_fetch(running, std::future::pending())
        .is_some());
    let mut roots = vec![slot(scheduled, "scheduled"), slot(running, "running")];
    roots.sort_by(|a, b| a.root_cid.cmp(&b.root_cid));
    assert_eq!(
        shutdown.pending_dag_fetch_slots(),
        PendingDagFetchSlots {
            limit: 2,
            held: 2,
            scheduled: 1,
            running: 1,
            roots,
            ..Default::default()
        }
    );

    let merge = shutdown
        .claim_pending_dag_merge(scheduled)
        .expect("a scheduled root is claimed by its merge");
    let slots = shutdown.pending_dag_fetch_slots();
    assert_eq!((slots.held, slots.scheduled, slots.merging), (2, 0, 1));
    assert!(slots.roots.contains(&slot(scheduled, "merging")));

    drop(merge);
    let slots = shutdown.pending_dag_fetch_slots();
    assert_eq!((slots.held, slots.merging), (1, 0));
    assert_eq!(slots.roots, vec![slot(running, "running")]);
    shutdown.shutdown().await;
}
