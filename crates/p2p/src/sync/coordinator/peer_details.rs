//! Everything the sync layer knows about one peer, for debugging.

use blockstore::Blockstore;
use chrono::DateTime;

use super::SyncCoordinator;
use crate::sync::peer_state::{PeerHistory, Timestamp};
use crate::sync::push_backlog::PeerBacklogSnapshot;
use crate::transport::{P2PTransport, PeerConnectionInfo, PeerFetchStats, PeerId};

/// Longest list of pending or quarantined roots a details call returns.
const PEER_DETAILS_ROOT_LIMIT: usize = 50;

/// Everything this node knows about one peer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PeerDetails {
    pub peer_id: String,
    /// Connection history and sync outcomes; empty when the peer is untracked.
    #[serde(flatten)]
    pub history: PeerHistory,
    /// The live connection, when connected and the transport can report it.
    pub connection: Option<PeerConnectionInfo>,
    /// Collections this node pushes to the peer as a replicator.
    pub replicated_collections: Vec<String>,
    /// This node's outbound push queue for the peer, when it holds any work.
    pub push_queue: Option<PeerBacklogSnapshot>,
    /// Incoming DAGs the peer announced that are still missing blocks.
    pub pending_fetch_count: usize,
    /// Up to 50 of them.
    pub pending_fetches: Vec<PendingFetch>,
    /// Roots from the peer that failed to merge deterministically, up to 50.
    pub quarantined: Vec<QuarantinedRoot>,
    /// CAR fetch measurements, for transports that keep them.
    pub car_fetches: Option<PeerFetchStats>,
    /// Sources that could not be read, so their empty fields mean unknown.
    pub source_errors: Vec<String>,
}

/// One incoming DAG still being fetched from the peer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingFetch {
    pub root_cid: String,
    pub doc_id: String,
    pub collection_id: String,
    pub missing_blocks: usize,
    pub attempts: u32,
    pub fetch_failures: u32,
    pub last_fetch_error: Option<String>,
    pub next_retry_in_ms: u64,
}

/// One root from the peer that will not merge until its cause is fixed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QuarantinedRoot {
    pub root_cid: String,
    pub doc_id: String,
    pub collection_id: String,
    pub reason: String,
    pub quarantined_at: Option<Timestamp>,
}

impl<B: Blockstore + 'static, T: P2PTransport> SyncCoordinator<B, T> {
    /// Everything this node knows about one peer. A source that fails is
    /// named in `source_errors` and the rest are still returned.
    pub async fn peer_details(&self, peer_id: &PeerId) -> PeerDetails {
        let peer = peer_id.as_str();
        let mut source_errors = Vec::new();
        let connection = self
            .runtime
            .transport
            .peer_connection(peer_id)
            .await
            .unwrap_or_else(|error| {
                source_errors.push(format!("connection: {error}"));
                None
            });
        let car_fetches = match self.runtime.transport.fetch_stats().await {
            Ok(stats) => stats.into_iter().find(|stats| stats.peer == peer),
            Err(error) => {
                source_errors.push(format!("car_fetches: {error}"));
                None
            }
        };
        let push_queue = self
            .runtime
            .push_backlog
            .snapshot()
            .await
            .per_peer
            .into_iter()
            .find(|entry| entry.peer_id == peer);

        let now = crate::rt::time::Instant::now();
        let mut pending = self.manager.pending_dags_from(peer);
        pending.sort_by_key(|(_, dag)| dag.next_retry_at);
        let pending_fetch_count = pending.len();
        let pending_fetches = pending
            .into_iter()
            .take(PEER_DETAILS_ROOT_LIMIT)
            .map(|(cid, dag)| PendingFetch {
                root_cid: cid.to_string(),
                doc_id: dag.doc_id,
                collection_id: dag.collection_id,
                missing_blocks: dag.missing.len(),
                attempts: dag.attempts,
                fetch_failures: dag.fetch_failures,
                last_fetch_error: dag.last_fetch_error,
                next_retry_in_ms: dag.next_retry_at.saturating_duration_since(now).as_millis()
                    as u64,
            })
            .collect();

        let quarantined = match self.manager.quarantined_from(peer).await {
            Ok(records) => records
                .into_iter()
                .take(PEER_DETAILS_ROOT_LIMIT)
                .map(|(cid, dag)| QuarantinedRoot {
                    root_cid: cid.to_string(),
                    doc_id: dag.record.doc_id,
                    collection_id: dag.record.collection_id,
                    reason: dag.reason,
                    quarantined_at: DateTime::from_timestamp(
                        dag.quarantined_at_unix_secs as i64,
                        0,
                    ),
                })
                .collect(),
            Err(error) => {
                source_errors.push(format!("quarantined: {error}"));
                Vec::new()
            }
        };

        PeerDetails {
            peer_id: peer.to_string(),
            history: self.access.peer_state.history(peer).unwrap_or_default(),
            connection,
            replicated_collections: self.access.replicators.get_collections(peer),
            push_queue,
            pending_fetch_count,
            pending_fetches,
            quarantined,
            car_fetches,
            source_errors,
        }
    }
}
