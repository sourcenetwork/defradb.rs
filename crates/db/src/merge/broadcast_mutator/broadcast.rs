//! Broadcast helper functions for asynchronous P2P propagation.

use std::sync::Arc;

use blockstore::Blockstore;
use events::{Bus, Message, P2PBroadcastDroppedData, P2PNoPeersData};
use p2p::sync::{BroadcastResult, SyncCoordinator};
use p2p::transport::P2PTransport;
use query::mutator::BroadcastStatus;

use crate::block::builder::BlockResult;

pub(crate) const BROADCAST_MAX_RETRIES: u32 = 10;
const BROADCAST_MAX_RETRY_DELAY_MS: u64 = 10_000;

fn is_expected_fire_and_forget_failure(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("insufficientpeers")
        || lower.contains("channel send error")
        || lower.contains("channel receive error")
        || p2p::error::Error::is_connection_loss_reason(&lower)
}

/// A root a committed write announces over gossip: its CID, document ID
/// (empty for a collection block), collection ID and block length.
pub(crate) type AnnouncedRoot = (cid::Cid, String, String, usize);

/// What one pending announcement holds besides its block: its broadcast
/// envelope, IDs, signature and coalescer entry.
const ANNOUNCEMENT_OVERHEAD_BYTES: usize = 512;

/// Hands a committed write's gossip to the bounded pending-gossip budget,
/// waiting for room while it is full. If the wait deadline passes first,
/// every root the work would have announced is reported as
/// `p2p-broadcast-dropped`, since gossip never re-announces it (#2489).
pub(crate) async fn spawn_gossip_or_report<B, T, F>(
    sync: &SyncCoordinator<B, T>,
    bus: Option<&Arc<dyn Bus>>,
    task_name: &'static str,
    roots: Vec<AnnouncedRoot>,
    work: F,
) where
    B: Blockstore + 'static,
    T: P2PTransport,
    F: std::future::Future<Output = ()> + defra_core::thread_bounds::MaybeSend + 'static,
{
    let bytes = roots
        .iter()
        .map(|(.., block_len)| block_len + ANNOUNCEMENT_OVERHEAD_BYTES)
        .sum();
    if sync
        .spawn_non_authoritative_broadcast_task(task_name, bytes, work)
        .await
        .is_ok()
    {
        return;
    }
    for (cid, doc_id, collection_id, _) in roots {
        report_dropped(bus, cid, doc_id, collection_id);
    }
}

/// Tells subscribers a committed write's root was never announced.
fn report_dropped(
    bus: Option<&Arc<dyn Bus>>,
    cid: cid::Cid,
    doc_id: String,
    collection_id: String,
) {
    if let Some(bus) = bus {
        bus.publish(Message::p2p_broadcast_dropped(P2PBroadcastDroppedData {
            cid,
            doc_id,
            collection_id,
        }));
    }
}

/// The collection topic's part of a publish error. When both topics
/// fail, the broadcaster joins their errors as `doc=…, collection=…`.
fn collection_topic_error(error: &str) -> &str {
    error
        .split_once("collection=")
        .map_or(error, |(_, collection)| collection)
}

/// Tells subscribers a push reached no peer on the collection topic, as Go
/// Shinzo's `P2PNoPeers` does, naming the document the push was for.
fn emit_no_peers(bus: Option<&Arc<dyn Bus>>, error: &str, doc_id: &str, collection_id: &str) {
    let no_peers = collection_topic_error(error).contains("InsufficientPeers");
    if let Some(bus) = bus.filter(|_| no_peers) {
        bus.publish(Message::p2p_no_peers(P2PNoPeersData {
            doc_id: doc_id.to_string(),
            collection_id: collection_id.to_string(),
            topic: p2p::DefraTopic::collection(collection_id).to_string(),
        }));
    }
}

pub fn broadcast_retry_delay_ms(
    err_str: &str,
    connected_peers: usize,
    attempt: u32,
) -> Option<u64> {
    if !err_str.contains("InsufficientPeers") {
        return None;
    }
    if connected_peers == 0 || attempt > BROADCAST_MAX_RETRIES {
        return None;
    }
    let base_delay_ms = 100 * (1u64 << attempt.min(5));
    let peer_multiplier = match connected_peers {
        1 => 4,
        2 => 2,
        _ => 1,
    };
    Some(
        base_delay_ms
            .saturating_mul(peer_multiplier)
            .min(BROADCAST_MAX_RETRY_DELAY_MS),
    )
}

async fn connected_peer_count<B: Blockstore + 'static, T: P2PTransport>(
    sync: &SyncCoordinator<B, T>,
) -> usize {
    sync.transport()
        .connected_peers()
        .await
        .map(|peers| peers.len())
        .unwrap_or_else(|_| sync.peer_state().stats().connected_peers())
}

/// Log broadcast failures for asynchronous propagation paths.
pub(crate) fn log_broadcast_failure(status: &BroadcastStatus) {
    if let BroadcastStatus::Failed(err) = status {
        if is_expected_fire_and_forget_failure(err) {
            tracing::warn!(
                error = %err,
                "Fire-and-forget broadcast could not reach peers; document committed locally but was not replicated"
            );
        } else {
            tracing::error!(
                error = %err,
                "Fire-and-forget broadcast failed — document committed locally but NOT replicated"
            );
        }
    }
}

/// Broadcast via GossipSub with retry, optionally overriding the creator DID.
pub(crate) async fn broadcast_with_retry_with_creator<B: Blockstore + 'static, T: P2PTransport>(
    sync: &SyncCoordinator<B, T>,
    bus: Option<&Arc<dyn Bus>>,
    origin_doc_id: &str,
    block_result: &BlockResult,
    collection_id: &str,
    collection_name: &str,
    creator_override: Option<&str>,
) -> BroadcastStatus {
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        match sync
            .broadcast_local_update_with_creator(
                &block_result.cid,
                &block_result.block,
                &block_result.doc_id,
                collection_id,
                creator_override,
            )
            .await
        {
            Ok(BroadcastResult::Success) => {
                tracing::debug!(
                    doc_id = %block_result.doc_id,
                    cid = %block_result.cid,
                    collection = %collection_name,
                    attempts = attempt,
                    "Broadcast document to P2P network"
                );
                return BroadcastStatus::Success;
            }
            Ok(BroadcastResult::PartialDocumentOnly { collection_error }) => {
                let connected_peers = connected_peer_count(sync).await;
                if let Some(delay_ms) =
                    broadcast_retry_delay_ms(&collection_error, connected_peers, attempt)
                {
                    tracing::trace!(
                        doc_id = %block_result.doc_id,
                        attempt = attempt,
                        connected_peers = connected_peers,
                        delay_ms = delay_ms,
                        "Retrying broadcast after collection-topic InsufficientPeers"
                    );
                    sync.network()
                        .sleep(std::time::Duration::from_millis(delay_ms))
                        .await;
                    continue;
                }
                tracing::warn!(
                    doc_id = %block_result.doc_id,
                    collection = %collection_name,
                    error = %collection_error,
                    "Partial broadcast: document topic succeeded, collection topic failed"
                );
                emit_no_peers(bus, &collection_error, origin_doc_id, collection_id);
                return BroadcastStatus::Failed(format!(
                    "Partial: collection topic failed: {}",
                    collection_error
                ));
            }
            Ok(BroadcastResult::PartialCollectionOnly { document_error }) => {
                // Per-doc topics only have subscribers when a client explicitly
                // calls subscribe_document. For the common batch-create path
                // nobody is watching the doc-topic, and gossipsub returns
                // InsufficientPeers. Go's PublishToTopicAsync silently skips
                // when the topic has no local subscription, so for parity we
                // treat this as success: the collection-topic delivered to
                // subscribed replicators, which is the meaningful path.
                if document_error.contains("InsufficientPeers") {
                    tracing::debug!(
                        doc_id = %block_result.doc_id,
                        cid = %block_result.cid,
                        collection = %collection_name,
                        attempts = attempt,
                        "Doc-topic has no subscribers; broadcast delivered via collection topic"
                    );
                    return BroadcastStatus::Success;
                }
                tracing::warn!(
                    doc_id = %block_result.doc_id,
                    collection = %collection_name,
                    error = %document_error,
                    "Partial broadcast: collection topic succeeded, document topic failed"
                );
                return BroadcastStatus::Failed(format!(
                    "Partial: document topic failed: {}",
                    document_error
                ));
            }
            Ok(other) => {
                tracing::warn!(
                    doc_id = %block_result.doc_id,
                    collection = %collection_name,
                    result = ?other,
                    "Unexpected broadcast result from P2P network"
                );
                return BroadcastStatus::Failed(format!(
                    "Unexpected broadcast result: {:?}",
                    other
                ));
            }
            Err(e) => {
                let err_str = e.to_string();
                let connected_peers = connected_peer_count(sync).await;
                if let Some(delay_ms) = broadcast_retry_delay_ms(&err_str, connected_peers, attempt)
                {
                    tracing::trace!(
                        doc_id = %block_result.doc_id,
                        attempt = attempt,
                        connected_peers = connected_peers,
                        delay_ms = delay_ms,
                        "Retrying broadcast after InsufficientPeers"
                    );
                    sync.network()
                        .sleep(std::time::Duration::from_millis(delay_ms))
                        .await;
                    continue;
                }
                if err_str.contains("InsufficientPeers") && connected_peers == 0 {
                    tracing::debug!(
                        doc_id = %block_result.doc_id,
                        collection = %collection_name,
                        attempts = attempt,
                        "Skipping GossipSub retries because no P2P peers are connected"
                    );
                }
                tracing::warn!(
                    doc_id = %block_result.doc_id,
                    collection = %collection_name,
                    error = %e,
                    attempts = attempt,
                    "Failed to broadcast document to P2P network - local mutation succeeded"
                );
                emit_no_peers(bus, &err_str, origin_doc_id, collection_id);
                if err_str.contains(p2p::error::BROADCAST_SLOT_ELAPSED) {
                    report_dropped(
                        bus,
                        block_result.cid,
                        block_result.doc_id.clone(),
                        collection_id.to_string(),
                    );
                }
                return BroadcastStatus::Failed(e.to_string());
            }
        }
    }
}

#[cfg(test)]
mod no_peers_tests {
    use super::collection_topic_error;

    #[test]
    fn only_the_collection_topic_error_decides() {
        let doc_only = "failed to publish to both topics: doc=gossipsub publish error: \
                        InsufficientPeers, collection=channel send error";
        assert!(!collection_topic_error(doc_only).contains("InsufficientPeers"));
        let collection = "failed to publish to both topics: doc=channel send error, \
                          collection=gossipsub publish error: InsufficientPeers";
        assert!(collection_topic_error(collection).contains("InsufficientPeers"));
        let single = "gossipsub publish error: InsufficientPeers";
        assert_eq!(collection_topic_error(single), single);
    }
}
