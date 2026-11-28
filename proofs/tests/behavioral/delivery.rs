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
    addr, churn, connect, create, eventually, heads, heads_acked_by, holds, is_connected,
    markers_for, peer_id, replicate, row, running_slots, set, start, sync_status, COLLECTION,
    START_TIMEOUT,
};
use defra_harness::{DefraClient, TestCluster};
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
const DELIVERY_BOUND: Duration = Duration::from_secs(p2p::PENDING_RECOVERY_WORST_CASE_SECS);
/// How long A's head may wait for a slot a productive catch-up attempt holds:
/// the attempt gives it back at the first window boundary past its quantum,
/// and a window lasts at most the block-sync watchdog.
const SLOT_RELEASE_BOUND: Duration =
    p2p::CLOCK_ATTEMPT_QUANTUM.saturating_add(p2p::BLOCK_SYNC_COMPLETION_WATCHDOG);
/// How soon an unthrottled sender sees its head acknowledged, so a late
/// delivery is the receiver's.
const ACK_BOUND: Duration = Duration::from_secs(5);
/// How often the premise reads the hub's slots while the catch-up node runs:
/// well inside the time one catch-up history takes to fetch.
const LOAD_POLL: Duration = Duration::from_millis(20);
/// How far apart the two reads of a frozen hold are.
const HOLD_SETTLE: Duration = Duration::from_millis(250);
const DEFAULT_FETCH_SLOTS: usize = 4;
const TTL_SECS: u64 = 20;
/// How long B holds the head's only provider paused: past the TTL.
const PAUSE: Duration = Duration::from_secs(TTL_SECS + 10);

const SOURCE: usize = 0;
const PEER: usize = 1;
const RECEIVER: usize = 2;
const SIBLING_DOCS: usize = 10;

/// The hub was down while the catch-up node wrote a long history; the hub is
/// back and fetching that history from it. The catch-up node's replicator to
/// the hub is set again once the hub is back, so its install replays the whole
/// history at once rather than when a retry sweep next comes round. The
/// catch-up node runs freely until the hub has fetches of that history running
/// in its slots, and the scenario then freezes it (SIGSTOP): a fetch to a
/// frozen provider cannot finish, so the slots it holds stay held until the
/// scenario lets the catch-up node run again. The load is the catch-up node's
/// replicator replay alone, so every fetch to it is a clock-owned one. The
/// desktop only authors: it does not subscribe, so it neither receives nor
/// forwards the catch-up history, and nothing queues ahead of its own heads.
///
/// The hub is the model's receiver whose slots (`Slotted(r)`) hold the
/// catch-up node's share of its `K` slots, each `run` by a catch-up root, when
/// the desktop's head arrives.
struct CatchUp {
    docs: Vec<String>,
}

/// Bring the hub back under catch-up load and return, with the catch-up node
/// frozen, once `fetch_slots - 1` of the hub's slots (its per-peer share; one
/// with a single slot) run catch-up fetches. The hub knows no other roots, so
/// every running slot is the catch-up node's.
///
/// The hold is read while the catch-up node is frozen, from the hub's own
/// slot gauge, and only accepted once two reads `HOLD_SETTLE` apart agree: a
/// running fetch to a frozen provider cannot complete, only time out after the
/// transport's response bound, so the hold read then is the hold the desktop's
/// head meets. A hold that does not settle (an attempt that ended on bytes
/// already received, a deferred ingest) lets the catch-up node run again until
/// the hub reports the next one.
async fn hub_restarted_under_catchup(cluster: &mut TestCluster, fetch_slots: usize) -> CatchUp {
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
    let catchup_addr = addr(cluster, CATCHUP);
    cluster
        .client(CATCHUP)
        .p2p_replicator_delete(&[COLLECTION], Some(&addr(cluster, HUB)))
        .expect("stop replicating the catch-up node to the hub");

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
    connect(cluster, HUB, DESKTOP);
    let deadline = Instant::now() + Duration::from_secs(180);
    while !is_connected(cluster, HUB, &catchup_peer).await {
        assert!(
            Instant::now() < deadline,
            "the hub never reached the catch-up node"
        );
        let _ = cluster.client(HUB).p2p_connect(&[catchup_addr.as_str()]);
    }
    replicate(cluster, CATCHUP, HUB);

    let want = fetch_slots.saturating_sub(1).max(1);
    let loading = Instant::now();
    let deadline = loading + Duration::from_secs(180);
    let mut freezes = 0;
    loop {
        let status = sync_status(cluster, HUB).await;
        if running_slots(&status).len() >= want {
            cluster
                .pause_node(CATCHUP)
                .expect("freeze the catch-up node");
            freezes += 1;
            let first = running_slots(&sync_status(cluster, HUB).await);
            nuclei::time::sleep(HOLD_SETTLE).await;
            let settled = running_slots(&sync_status(cluster, HUB).await);
            if settled.len() >= want && settled == first {
                eprintln!(
                    "the frozen catch-up node holds {} of the hub's {fetch_slots} fetch slots, \
                     {:?} after the hub reached it, on freeze {freezes}",
                    settled.len(),
                    loading.elapsed()
                );
                return CatchUp { docs };
            }
            cluster
                .resume_node(CATCHUP)
                .expect("resume the catch-up node");
        }
        assert!(
            Instant::now() < deadline,
            "the hub's {fetch_slots} fetch slots never held {want} running catch-up fetches \
             while the catch-up node was frozen: the scenario cannot hold capacity. hub sync \
             status: {status}"
        );
        nuclei::time::sleep(LOAD_POLL).await;
    }
}

/// Wait until the desktop's pushes acked by the hub pass `before` and it owes
/// the hub nothing.
async fn wait_for_ack(cluster: &TestCluster, hub_peer: &str, before: u64, within: Duration) {
    let deadline = Instant::now() + within;
    while heads_acked_by(cluster, DESKTOP, hub_peer).await <= before
        || markers_for(cluster, DESKTOP, hub_peer).await > 0
    {
        assert!(
            Instant::now() < deadline,
            "the hub did not acknowledge the desktop's head within {within:?}: the push is \
             delayed before the hub's fetch slots matter"
        );
        nuclei::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A: a hub restarted under catch-up load still delivers a new document from
/// an unthrottled desktop within the bound. Once the load holds the hub's
/// slots, the catch-up node runs 100 ms in every 25 s: its connection stays up
/// and every fetch window sees it, but a window is too short to serve a whole
/// history, so its fetches stay productive and do not finish on their own.
#[nuclei::test(threads = 2)]
async fn hub_restart_under_catchup_delivers_new_doc_within_bound() {
    edge::compat(async move {
        let mut cluster = start(3, |builder| builder).await;
        let hub_peer = peer_id(&cluster, HUB);
        let load = hub_restarted_under_catchup(&mut cluster, DEFAULT_FETCH_SLOTS).await;
        let throttle = cluster
            .throttle_node(CATCHUP, THROTTLE_RUN, THROTTLE_STOP)
            .expect("throttle the catch-up node");
        let (hub, desktop) = (cluster.client(HUB), cluster.client(DESKTOP));

        let acked_before = heads_acked_by(&cluster, DESKTOP, &hub_peer).await;
        let x = create(&desktop, "x");
        let written = Instant::now();
        wait_for_ack(&cluster, &hub_peer, acked_before, ACK_BOUND).await;
        let acked = written.elapsed();
        let delivered = eventually(SLOT_RELEASE_BOUND.saturating_sub(written.elapsed()), || {
            holds(&hub, &desktop, &x)
        })
        .await;
        assert!(
            delivered,
            "the hub acknowledged the desktop's new document but did not hold it {:?} after it \
             was written; the catch-up node's fetches held every slot. hub sync status: {}",
            written.elapsed(),
            sync_status(&cluster, HUB).await
        );
        eprintln!(
            "A: x acked {acked:?} and delivered {:?} after it was written",
            written.elapsed()
        );

        throttle.release().await.expect("release the catch-up node");
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
/// fetch slot, held by a fetch to the frozen catch-up node when the head is
/// acknowledged; the desktop, the head's only provider, is then paused for
/// longer than the hub's 20 s TTL, so no fetch of it can finish before the TTL
/// passes. Only then does the catch-up node run again.
async fn acked_head_past_pending_ttl(restart_hub: bool) {
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
    hub_restarted_under_catchup(&mut cluster, 1).await;
    let desktop = cluster.client(DESKTOP);

    let acked_before = heads_acked_by(&cluster, DESKTOP, &hub_peer).await;
    let x = create(&desktop, "x");
    wait_for_ack(&cluster, &hub_peer, acked_before, Duration::from_secs(30)).await;
    let want = support::commit_cids(&desktop, &x);
    cluster.pause_node(DESKTOP).expect("pause the desktop");
    let paused = Instant::now();
    let hub = cluster.client(HUB);
    assert_ne!(
        support::commit_cids(&hub, &x),
        want,
        "the hub fetched the head before its provider paused: the catch-up load did not hold \
         the fetch slot, so the scenario never crosses the TTL"
    );
    cluster
        .resume_node(CATCHUP)
        .expect("resume the catch-up node");
    nuclei::time::sleep(PAUSE.saturating_sub(paused.elapsed())).await;

    if restart_hub {
        cluster
            .restart_node(HUB, START_TIMEOUT)
            .await
            .expect("restart the hub");
    }
    cluster.resume_node(DESKTOP).expect("resume the desktop");
    let resumed = Instant::now();
    connect(&cluster, HUB, DESKTOP);
    if restart_hub {
        connect(&cluster, HUB, CATCHUP);
    }

    let hub = cluster.client(HUB);
    let bound = Duration::from_secs(TTL_SECS) + DELIVERY_BOUND + Duration::from_secs(30);
    while support::commit_cids(&hub, &x) != want {
        assert_eq!(
            markers_for(&cluster, DESKTOP, &hub_peer).await,
            0,
            "the desktop owes the hub its head again: the ack did not stand"
        );
        assert!(
            resumed.elapsed() < bound,
            "the hub acknowledged the desktop's head and did not deliver it within {bound:?} \
             of its provider resuming; nobody owes it any more. hub sync status: {}",
            sync_status(&cluster, HUB).await
        );
        nuclei::time::sleep(Duration::from_millis(250)).await;
    }
    eprintln!(
        "B: x delivered {:?} after its provider resumed",
        resumed.elapsed()
    );
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
