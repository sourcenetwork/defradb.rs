//! Bitswap sync and cancel command handling.

use std::sync::Arc;

use ::bitswap::Store;
use cid::Cid;
use libp2p::PeerId;
use tracing::debug;

use crate::error::Result;
use crate::host::event::HostEvent;
use crate::QueryId;

use super::super::p2p_host::{BitswapQuery, P2PHost};

impl<S: Store> P2PHost<S> {
    /// Handle a Bitswap sync command.
    pub(super) async fn handle_bitswap_sync(
        &mut self,
        cid: Cid,
        providers: Vec<PeerId>,
        missing: Vec<Cid>,
        response: tokio::sync::oneshot::Sender<Result<QueryId>>,
    ) {
        // Generate a query ID for tracking
        static QUERY_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let query_id = QueryId(QUERY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed));

        debug!(
            cid = %cid,
            providers = ?providers,
            missing_count = missing.len(),
            query_id = query_id.0,
            "Starting Bitswap fetch"
        );

        let event_tx = self.event_tx.clone();
        let missing_cids: Vec<Cid> = missing;

        // Any connected peer may have the block in a small network, so the
        // given providers are widened with every connected peer.
        let mut providers_list = providers;
        for peer in self.peer_addrs.keys() {
            if !providers_list.contains(peer) {
                providers_list.push(*peer);
            }
        }

        let (fetch_id, mut receiver) = self
            .swarm
            .behaviour_mut()
            .bitswap
            .fetch(missing_cids.clone(), providers_list);
        let queries = Arc::clone(&self.bitswap_queries);

        // The fetch task removes its own registry entry when it ends, so a task
        // that finished before the parent registered it would have that entry
        // re-added with nothing left to remove it: the map would grow by one per
        // fetch, and a long-gone query would still report as cancellable. Gate
        // the task on registration having happened.
        let (registered_tx, registered_rx) = tokio::sync::oneshot::channel::<()>();

        // Spawn async task to fetch blocks (with cancellation support)
        let task_handle = tokio::spawn(async move {
            let _ = registered_rx.await;

            #[cfg(feature = "test-utils")]
            crate::testutil::block_started_fetch();

            let mut fetched = 0;

            // Timeout per block: 10 seconds. If no block arrives within this
            // window, we give up and report partial success so the retry
            // mechanism can try again with fresh provider info.
            let per_block_timeout = std::time::Duration::from_secs(10);

            loop {
                match tokio::time::timeout(per_block_timeout, receiver.recv()).await {
                    Ok(Some(block)) => {
                        fetched += 1;

                        // Send block to coordinator for storage
                        if let Err(e) = event_tx
                            .send(HostEvent::BitswapBlockReceived {
                                query_id,
                                cid: *block.cid(),
                                data: block.data().to_vec(),
                            })
                            .await
                        {
                            tracing::warn!(
                                query_id = query_id.0,
                                error = %e,
                                "Failed to send BitswapBlockReceived event"
                            );
                        }

                        if fetched == missing_cids.len() {
                            break;
                        }
                    }
                    Ok(None) => {
                        tracing::debug!("Bitswap channel closed, no more blocks");
                        break;
                    }
                    Err(_) => {
                        tracing::warn!(
                            fetched = fetched,
                            total = missing_cids.len(),
                            "Bitswap timeout waiting for block"
                        );
                        break;
                    }
                }
            }

            let success = fetched == missing_cids.len();
            debug!(
                query_id = query_id.0,
                fetched = fetched,
                total = missing_cids.len(),
                success = success,
                "Bitswap fetch complete"
            );
            // Dropping the receiver is enough on a timeout: the fetch in the
            // behaviour ends once its requests resolve or time out.
            drop(receiver);

            let _ = event_tx
                .send(HostEvent::BitswapComplete {
                    query_id,
                    success,
                    error: if success {
                        None
                    } else {
                        Some(format!(
                            "Only fetched {} of {} blocks",
                            fetched,
                            missing_cids.len()
                        ))
                    },
                })
                .await;
            queries.remove(&query_id);
        });

        #[cfg(feature = "test-utils")]
        crate::testutil::stall_before_query_registration().await;

        self.bitswap_queries.insert(
            query_id,
            BitswapQuery {
                abort: task_handle.abort_handle(),
                fetch_id,
            },
        );
        let _ = registered_tx.send(());

        if response.send(Ok(query_id)).is_err() {
            debug!(cid = %cid, "BitswapSync command response dropped - caller cancelled");
        }
    }

    pub(super) fn handle_bitswap_cancel(
        &mut self,
        query_id: QueryId,
        response: tokio::sync::oneshot::Sender<bool>,
    ) {
        let cancelled = if let Some(query) = self.bitswap_queries.remove(&query_id) {
            debug!(query_id = ?query_id, "Cancelling Bitswap query");
            query.abort.abort();
            self.swarm.behaviour_mut().bitswap.cancel(query.fetch_id);
            true
        } else {
            debug!(query_id = ?query_id, "Bitswap query not found for cancellation");
            false
        };
        if response.send(cancelled).is_err() {
            debug!(query_id = ?query_id, "BitswapCancel command response dropped - caller cancelled");
        }
    }
}
