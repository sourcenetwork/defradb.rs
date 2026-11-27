use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

use super::{PendingDagFetchSlot, PendingDagFetchSlots};
use crate::network::test_network;
use crate::sync::coordinator::SyncShutdownHandle;

fn root(name: &[u8]) -> Cid {
    Cid::new_v1(0x55, Code::Sha2_256.digest(name))
}

fn slot(root_cid: Cid, state: &'static str, peer: Option<&str>) -> PendingDagFetchSlot {
    PendingDagFetchSlot {
        root_cid: root_cid.to_string(),
        state,
        peer: peer.map(str::to_owned),
    }
}

/// Each slot is reported under the state that holds it and the peer whose
/// root it serves, and leaves the gauge when its owner releases it.
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

    assert!(shutdown.reserve_pending_dag_fetch(scheduled, Some("peer-a")));
    assert!(shutdown.spawn_pending_dag_fetch(running, Some("peer-b"), std::future::pending()));
    let mut roots = vec![
        slot(scheduled, "scheduled", Some("peer-a")),
        slot(running, "running", Some("peer-b")),
    ];
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
    assert!(
        slots
            .roots
            .contains(&slot(scheduled, "merging", Some("peer-a"))),
        "the claimed slot keeps the peer its reservation named"
    );

    drop(merge);
    let slots = shutdown.pending_dag_fetch_slots();
    assert_eq!((slots.held, slots.merging), (1, 0));
    assert_eq!(slots.roots, vec![slot(running, "running", Some("peer-b"))]);
    shutdown.shutdown().await;
}

/// A root no caller named a source for reports no peer, and a reservation
/// the clock named a peer for keeps that peer when its fetch claims it.
#[nuclei::test(threads = 0)]
async fn a_slot_reports_the_peer_its_root_came_from() {
    let shutdown = SyncShutdownHandle::new(2, test_network());
    let (unnamed, claimed) = (root(b"unnamed"), root(b"claimed"));

    assert!(shutdown.spawn_pending_dag_fetch(unnamed, None, std::future::pending()));
    let slots = shutdown.pending_dag_fetch_slots();
    assert_eq!(slots.roots, vec![slot(unnamed, "running", None)]);

    assert!(shutdown.reserve_pending_dag_fetch(claimed, Some("peer-c")));
    let root_order = |state: &'static str, peer: Option<&str>| {
        let mut roots = vec![slot(claimed, state, peer), slot(unnamed, "running", None)];
        roots.sort_by(|a, b| a.root_cid.cmp(&b.root_cid));
        roots
    };
    assert_eq!(
        shutdown.pending_dag_fetch_slots().roots,
        root_order("scheduled", Some("peer-c"))
    );
    assert!(shutdown.spawn_pending_dag_fetch(claimed, None, std::future::pending()));
    assert_eq!(
        shutdown.pending_dag_fetch_slots().roots,
        root_order("running", Some("peer-c")),
        "the clock's reservation names the peer the claim keeps"
    );
    shutdown.shutdown().await;
}
