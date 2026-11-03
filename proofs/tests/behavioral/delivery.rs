//! P2P delivery liveness: a head one node commits reaches every connected,
//! subscribed peer, within a bound once they are connected and fetch capacity
//! is free. A head the receiver has acknowledged is merged, durably pending,
//! or an ancestor of one; the sender never has to send it again.
//!
//! Model: `proofs/tla/P2PDelivery.tla`, `LIVE_HeadDelivered` and
//! `INV_ObligationOwned`.
//!
//! A scenario today's code fails is `#[ignore]`d with the finding that fails
//! it. It still asserts the whole property, so it passes when that finding is
//! fixed and not before.

#[path = "delivery_cluster.rs"]
mod cluster;

use crate::support;
use cluster::{
    addr, churn, connect, create, eventually, fetches_in_flight_to, heads, heads_acked_by, holds,
    markers_for, peer_id, pending_from, replicate, row, set, start, sync_status, COLLECTION,
    START_TIMEOUT,
};
use defra_harness::{DefraClient, TestCluster, Throttle};
use std::time::{Duration, Instant};

const HUB: usize = 0;
const CATCHUP: usize = 1;
const DESKTOP: usize = 2;
const CATCHUP_DOCS: usize = 6;
const CATCHUP_UPDATES: usize = 400;
const THROTTLE_RUN: Duration = Duration::from_millis(100);
const THROTTLE_STOP: Duration = Duration::from_secs(25);
/// `p2p::PENDING_RECOVERY_WORST_CASE_SECS`: how long a pending head may wait
/// for its fetch once the receiver has capacity for it.
const DELIVERY_BOUND: Duration = Duration::from_secs(60);
const DEFAULT_FETCH_SLOTS: u64 = 4;
const TTL_SECS: u64 = 20;

const SOURCE: usize = 0;
const PEER: usize = 1;
const RECEIVER: usize = 2;
const SIBLING_DOCS: usize = 10;

/// The hub was down while the catch-up node wrote a long history; the hub is
/// back and fetching that history from it. The catch-up node runs 100 ms in
/// every 25 s: its connection stays up and every fetch window sees it, but a
/// window is too short to serve a whole history, so its fetches do not finish
/// on their own. The desktop only authors: it does not subscribe, so it
/// neither receives nor forwards the catch-up history, and nothing queues
/// ahead of its own heads.
struct CatchUp {
    docs: Vec<String>,
    throttle: Throttle,
}

/// Bring the hub back under catch-up load and return once the catch-up node's
/// roots fill the hub's `fetch_slots`: as many are pending as there are slots,
/// and as many are in flight as its per-peer share allows (one slot fewer).
async fn hub_restarted_under_catchup(cluster: &mut TestCluster, fetch_slots: u64) -> CatchUp {
    cluster
        .client(DESKTOP)
        .p2p_collection_delete(&[COLLECTION])
        .expect("unsubscribe the desktop");
    connect(cluster, CATCHUP, HUB);
    connect(cluster, DESKTOP, HUB);
    replicate(cluster, CATCHUP, HUB);
    replicate(cluster, DESKTOP, HUB);
    for source in [CATCHUP, DESKTOP] {
        let (hub, from) = (cluster.client(HUB), cluster.client(source));
        let doc = create(&from, &format!("baseline_{source}"));
        assert!(
            eventually(Duration::from_secs(30), || holds(&hub, &from, &doc)).await,
            "node{source}'s baseline document did not reach the hub"
        );
    }
    let catchup_peer = peer_id(cluster, CATCHUP);

    let stopped = cluster.stop_node(HUB).await.expect("stop the hub");
    let catchup = cluster.client(CATCHUP);
    let docs: Vec<String> = (0..CATCHUP_DOCS)
        .map(|i| {
            let doc = create(&catchup, &format!("catchup_{i}"));
            churn(&catchup, &doc, CATCHUP_UPDATES);
            doc
        })
        .collect();
    cluster
        .start_stopped_node(stopped, START_TIMEOUT)
        .await
        .expect("restart the hub");
    connect(cluster, HUB, CATCHUP);
    connect(cluster, HUB, DESKTOP);
    let throttle = cluster
        .throttle_node(CATCHUP, THROTTLE_RUN, THROTTLE_STOP)
        .expect("throttle the catch-up node");

    let hub = cluster.client(HUB);
    let ids = docs.clone();
    tokio::task::spawn_blocking(move || {
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let _ = hub.p2p_document_sync(COLLECTION, &ids);
    });

    let per_peer = fetch_slots.saturating_sub(1).max(1);
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let pending = pending_from(cluster, HUB, &catchup_peer).await;
        let in_flight = fetches_in_flight_to(cluster, HUB, &catchup_peer).await;
        if pending >= fetch_slots && in_flight >= per_peer {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the catch-up load never filled the hub's {fetch_slots} fetch slots \
             (pending from it {pending}, in flight to it {in_flight}): the scenario cannot hold capacity"
        );
        nuclei::time::sleep(Duration::from_secs(1)).await;
    }
    CatchUp { docs, throttle }
}

/// A: a hub restarted under catch-up load still delivers a new document from
/// an unthrottled desktop within the bound.
#[nuclei::test(threads = 2)]
#[ignore = "expected red until stalled catch-up fetches stop holding every fetch slot (#2548); run with --ignored"]
async fn hub_restart_under_catchup_delivers_new_doc_within_bound() {
    let mut cluster = start(3, |builder| builder).await;
    let load = hub_restarted_under_catchup(&mut cluster, DEFAULT_FETCH_SLOTS).await;
    let (hub, desktop) = (cluster.client(HUB), cluster.client(DESKTOP));

    let x = create(&desktop, "x");
    let written = Instant::now();
    let delivered = eventually(DELIVERY_BOUND, || holds(&hub, &desktop, &x)).await;
    assert!(
        delivered,
        "the desktop's new document was not on the hub {:?} after it was written; \
         the catch-up node's fetches held every slot. hub sync status: {}",
        written.elapsed(),
        sync_status(&cluster, HUB).await
    );

        load.throttle
            .release()
            .await
            .expect("release the catch-up node");
        let catchup = cluster.client(CATCHUP);
        assert!(
            eventually(Duration::from_secs(15 * 60), || load
                .docs
                .iter()
                .all(|doc| holds(&hub, &catchup, doc)))
            .await,
            "the catch-up history did not converge on the hub within 15 minutes of the throttle lifting"
        );
    })
    .await
}

/// B: a head the hub acknowledged and then held past the pending-DAG TTL is
/// still delivered, and its sender never has to send it again. The hub has one
/// fetch slot and a 20 s TTL, so the catch-up load holds the head past it.
async fn acked_head_past_pending_ttl(restart_hub: bool) {
    let ttl = Duration::from_secs(TTL_SECS);
    let ttl_flag = TTL_SECS.to_string();
    let mut cluster = start(3, |builder| {
        builder.with_node_extra_rust_args(
            HUB,
            [
                "--p2p-pending-dag-ttl-secs",
                ttl_flag.as_str(),
                "--p2p-max-concurrent-dag-fetches",
                "1",
            ],
        )
    })
    .await;
    let hub_peer = peer_id(&cluster, HUB);
    let load = hub_restarted_under_catchup(&mut cluster, 1).await;
    let desktop = cluster.client(DESKTOP);

    let acked_before = heads_acked_by(&cluster, DESKTOP, &hub_peer).await;
    let x = create(&desktop, "x");
    let deadline = Instant::now() + Duration::from_secs(30);
    while heads_acked_by(&cluster, DESKTOP, &hub_peer).await <= acked_before
        || markers_for(&cluster, DESKTOP, &hub_peer).await > 0
    {
        assert!(
            Instant::now() < deadline,
            "the hub never acknowledged the desktop's head"
        );
        nuclei::time::sleep(Duration::from_millis(250)).await;
    }
    let acked = Instant::now();

    let hub = cluster.client(HUB);
    let held_until = acked + 3 * ttl;
    while Instant::now() < held_until {
        assert_eq!(
            markers_for(&cluster, DESKTOP, &hub_peer).await,
            0,
            "the desktop owes the hub its head again: the ack did not stand"
        );
        assert!(
            acked.elapsed() >= ttl || !holds(&hub, &desktop, &x),
            "the hub delivered the head {:?} after acking it, inside the {TTL_SECS} s TTL: \
             the catch-up load did not hold the fetch slot, so the scenario never crossed the TTL",
            acked.elapsed()
        );
        nuclei::time::sleep(Duration::from_secs(1)).await;
    }

    if restart_hub {
        cluster
            .restart_node(HUB, START_TIMEOUT)
            .await
            .expect("restart the hub");
        connect(&cluster, HUB, DESKTOP);
    }
    load.throttle
        .release()
        .await
        .expect("release the catch-up node");
    if restart_hub {
        connect(&cluster, HUB, CATCHUP);
    }

    let hub = cluster.client(HUB);
    let bound = ttl + Duration::from_secs(60) + Duration::from_secs(30);
    let released = Instant::now();
    while !holds(&hub, &desktop, &x) {
        assert_eq!(
            markers_for(&cluster, DESKTOP, &hub_peer).await,
            0,
            "the desktop owes the hub its head again: the ack did not stand"
        );
        assert!(
            released.elapsed() < bound,
            "the hub acknowledged the desktop's head and did not deliver it within {bound:?} \
             of fetch capacity returning; nobody owes it any more. hub sync status: {}",
            sync_status(&cluster, HUB).await
        );
        nuclei::time::sleep(Duration::from_millis(250)).await;
    }
}

#[nuclei::test(threads = 2)]
async fn acked_head_past_pending_ttl_is_delivered() {
    edge::compat(async move {
        acked_head_past_pending_ttl(false).await;
    })
    .await
}

/// B': the same, with the hub restarted while the head is past its TTL.
#[nuclei::test(threads = 2)]
async fn acked_head_past_pending_ttl_is_delivered_across_hub_restart() {
    edge::compat(async move {
        acked_head_past_pending_ttl(true).await;
    })
    .await
}

/// SIBLING_DOCS documents written on SOURCE and replayed to RECEIVER by a
/// replicator set afterwards; returns once SOURCE owes RECEIVER nothing.
///
/// With `siblings`, SOURCE and PEER edit every document while apart and PEER
/// then sends its edits to SOURCE, so each document ends with two concurrent
/// heads on SOURCE that the replay sends one after the other. Without it, the
/// same edits are made in order on SOURCE: one head each. SOURCE stops
/// replicating to PEER first, so only PEER owes anything when they meet again.
async fn replayed_to_receiver(siblings: bool) -> (TestCluster, Vec<String>) {
    let mut cluster = start(3, |builder| builder).await;
    connect(&cluster, SOURCE, PEER);
    replicate(&cluster, SOURCE, PEER);
    replicate(&cluster, PEER, SOURCE);
    let source = cluster.client(SOURCE);
    let docs: Vec<String> = (0..SIBLING_DOCS)
        .map(|i| create(&source, &format!("x_{i}")))
        .collect();
    let peer = cluster.client(PEER);
    assert!(
        eventually(Duration::from_secs(60), || docs
            .iter()
            .all(|doc| holds(&peer, &source, doc)))
        .await,
        "the peer did not receive the source's documents"
    );
    source
        .p2p_replicator_delete(&[COLLECTION], Some(&addr(&cluster, PEER)))
        .expect("stop replicating the source to the peer");

    if siblings {
        let stopped_peer = cluster.stop_node(PEER).await.expect("stop the peer");
        for doc in &docs {
            set(&cluster.client(SOURCE), doc, "status", "a");
        }
        let stopped_source = cluster.stop_node(SOURCE).await.expect("stop the source");
        cluster
            .start_stopped_node(stopped_peer, START_TIMEOUT)
            .await
            .expect("restart the peer");
        for doc in &docs {
            set(&cluster.client(PEER), doc, "note", "b1");
            set(&cluster.client(PEER), doc, "note", "b2");
        }
        cluster
            .start_stopped_node(stopped_source, START_TIMEOUT)
            .await
            .expect("restart the source");
        connect(&cluster, SOURCE, PEER);
    } else {
        for doc in &docs {
            set(&source, doc, "status", "a");
            set(&source, doc, "note", "b1");
            set(&source, doc, "note", "b2");
        }
    }
    let want_heads = if siblings { 2 } else { 1 };
    let (source, peer) = (cluster.client(SOURCE), cluster.client(PEER));
    assert!(
        eventually(Duration::from_secs(120), || docs.iter().all(|doc| {
            heads(&source, doc).len() == want_heads
                && support::commit_cids(&peer, doc).is_subset(&support::commit_cids(&source, doc))
        }))
        .await,
        "the source did not settle on {want_heads} head(s) per document holding the peer's edits"
    );

    let receiver_peer = peer_id(&cluster, RECEIVER);
    connect(&cluster, SOURCE, RECEIVER);
    replicate(&cluster, SOURCE, RECEIVER);
    let receiver = cluster.client(RECEIVER);
    let deadline = Instant::now() + Duration::from_secs(60);
    while docs.iter().any(|doc| row(&receiver, doc).is_null())
        || markers_for(&cluster, SOURCE, &receiver_peer).await > 0
    {
        assert!(
            Instant::now() < deadline,
            "the replay to the receiver did not finish: it lacks documents or the source still owes it heads"
        );
        nuclei::time::sleep(Duration::from_millis(250)).await;
    }
    (cluster, docs)
}

/// Documents the receiver does not hold as the source does, with what it has.
fn missing_on_receiver(
    source: &DefraClient,
    receiver: &DefraClient,
    docs: &[String],
) -> Vec<String> {
    docs.iter()
        .filter_map(|doc| {
            let got = row(receiver, doc);
            let complete = got["status"] == "a"
                && got["note"] == "b2"
                && heads(receiver, doc) == heads(source, doc)
                && holds(receiver, source, doc);
            (!complete).then(|| {
                format!(
                    "{doc}: status={} note={} heads {}/{}",
                    got["status"],
                    got["note"],
                    heads(receiver, doc).len(),
                    heads(source, doc).len()
                )
            })
        })
        .collect()
}

async fn receiver_converges(cluster: &TestCluster, docs: &[String], timeout: Duration) -> bool {
    let (source, receiver) = (cluster.client(SOURCE), cluster.client(RECEIVER));
    eventually(timeout, || {
        missing_on_receiver(&source, &receiver, docs).is_empty()
    })
    .await
}

/// C: both sibling heads of every document reach the receiver.
#[nuclei::test]
#[ignore = "expected red until a sibling head from one sender stops being acked as covered and dropped (audit F1); run with --ignored"]
async fn sibling_heads_both_delivered_by_replay() {
    edge::compat(async move {
        let (cluster, docs) = replayed_to_receiver(true).await;
        let converged = receiver_converges(&cluster, &docs, Duration::from_secs(60)).await;
        assert!(
            converged,
            "60 s after the source owed it nothing, the receiver lacks: {:#?}",
            missing_on_receiver(&cluster.client(SOURCE), &cluster.client(RECEIVER), &docs)
        );
    })
    .await
}

/// C's control: the same replay with one head per document delivers.
#[nuclei::test]
async fn single_head_docs_delivered_by_replay() {
    edge::compat(async move {
        let (cluster, docs) = replayed_to_receiver(false).await;
        let converged = receiver_converges(&cluster, &docs, Duration::from_secs(60)).await;
        assert!(
            converged,
            "60 s after the source owed it nothing, the receiver lacks: {:#?}",
            missing_on_receiver(&cluster.client(SOURCE), &cluster.client(RECEIVER), &docs)
        );
    })
    .await
}

/// D: a head the receiver acknowledged and dropped is found again when the
/// two reconnect, without setting the replicator again. The drop comes from
/// C's sibling replay.
#[nuclei::test]
#[ignore = "expected red until a reconnect rediscovers heads the receiver acked and dropped (audit F3, F34); run with --ignored"]
async fn acked_drop_is_rediscovered_on_reconnect() {
    edge::compat(async move {
        let (cluster, docs) = replayed_to_receiver(true).await;
        assert!(
            !receiver_converges(&cluster, &docs, Duration::from_secs(60)).await,
            "no acked drop to rediscover: the replay delivered every sibling head, \
             so this scenario needs another way to drop an acked head"
        );

        let receiver_addr = addr(&cluster, RECEIVER);
        cluster
            .p2p_disconnect(SOURCE, &[receiver_addr.as_str()])
            .await
            .expect("disconnect the source from the receiver");
        connect(&cluster, SOURCE, RECEIVER);

        let converged = receiver_converges(&cluster, &docs, Duration::from_secs(60)).await;
        assert!(
            converged,
            "60 s after reconnecting, the receiver still lacks: {:#?}",
            missing_on_receiver(&cluster.client(SOURCE), &cluster.client(RECEIVER), &docs)
        );
    })
    .await
}
