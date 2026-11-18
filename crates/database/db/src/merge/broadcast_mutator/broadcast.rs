//! Broadcast helper functions for asynchronous P2P propagation.

use std::sync::Arc;

use blockstore::Blockstore;
use events::{Bus, Message, P2PBroadcastDroppedData};
use p2p::sync::{BroadcastResult, SyncCoordinator};
use p2p::transport::P2PTransport;
use query::mutator::BroadcastStatus;

use crate::block::builder::BlockResult;

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

/// Announce a committed write over gossip once, optionally overriding the
/// creator DID.
///
/// Go publishes a write once, when it commits, and never again: "Retries are
/// for replicators only and should not pollute the pubsub network"
/// (internal/db/p2p/p2p.go SendUpdate). A topic with no subscriber then
/// reaches no one, and a peer that subscribes later learns of the write
/// through a later update's DAG (#2841).
pub async fn broadcast_with_creator<B, T>(
    sync: &SyncCoordinator<B, T>,
    bus: Option<&Arc<dyn Bus>>,
    block_result: &BlockResult,
    collection_id: &str,
    collection_name: &str,
    creator_override: Option<&str>,
) -> BroadcastStatus
where
    B: Blockstore + 'static,
    T: P2PTransport,
{
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
                "Broadcast document to P2P network"
            );
            BroadcastStatus::Success
        }
        Ok(BroadcastResult::PartialDocumentOnly { collection_error }) => {
            tracing::warn!(
                doc_id = %block_result.doc_id,
                collection = %collection_name,
                error = %collection_error,
                "Partial broadcast: document topic succeeded, collection topic failed"
            );
            BroadcastStatus::Failed(format!(
                "Partial: collection topic failed: {}",
                collection_error
            ))
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
            BroadcastStatus::Failed(format!(
                "Partial: document topic failed: {}",
                document_error
            ))
        }
        Ok(other) => {
            tracing::warn!(
                doc_id = %block_result.doc_id,
                collection = %collection_name,
                result = ?other,
                "Unexpected broadcast result from P2P network"
            );
            BroadcastStatus::Failed(format!("Unexpected broadcast result: {:?}", other))
        }
        Err(e) => {
            tracing::warn!(
                doc_id = %block_result.doc_id,
                collection = %collection_name,
                error = %e,
                "Failed to broadcast document to P2P network - local mutation succeeded"
            );
            if e.to_string().contains(p2p::error::BROADCAST_SLOT_ELAPSED) {
                report_dropped(
                    bus,
                    block_result.cid,
                    block_result.doc_id.clone(),
                    collection_id.to_string(),
                );
            }
            BroadcastStatus::Failed(e.to_string())
        }
    }
}
