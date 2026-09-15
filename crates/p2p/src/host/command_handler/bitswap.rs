//! Bitswap sync and cancel command handling.

use std::sync::Arc;

use cid::Cid;
use iroh_bitswap::Store;
use libp2p::PeerId;
use tracing::{debug, info, warn};

use crate::error::Result;
use crate::host::event::HostEvent;
use crate::QueryId;

use super::super::p2p_host::P2PHost;

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

        info!(
            cid = %cid,
            providers = ?providers,
            missing_count = missing.len(),
            query_id = query_id.0,
            "Starting Bitswap block fetch via Client API"
        );
        debug!(
            cid = %cid,
            providers = ?providers,
            missing_count = missing.len(),
            "Starting Bitswap fetch"
        );

        // Clone the client for use in the spawned task
        let client = self.swarm.behaviour().bitswap.client().clone();
        let event_tx = self.event_tx.clone();
        let missing_cids: Vec<Cid> = missing;
        let providers_list = providers;

        // The session manager keeps every session, with its worker task and
        // queues, until `Session::stop`; the fetch task stops its own session
        // when it finishes, and the cancel path stops it by id (`bitswap.rs`
        // `handle_bitswap_cancel`), so a session lives exactly as long as
        // its query.
        let session = client.new_session().await;
        let session_id = session.id();
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

            // Add each provider for each missing CID
            for cid in &missing_cids {
                for provider in &providers_list {
                    session.add_provider(cid, *provider).await;
                }
            }

            tracing::debug!(
                count = missing_cids.len(),
                "Bitswap session created, calling get_blocks"
            );
            match session.get_blocks(&missing_cids).await {
                Ok(receiver) => {
                    tracing::debug!("Bitswap get_blocks returned receiver, waiting for blocks");
                    // Use into_parts() to get the underlying channel
                    // BlockReceiver only implements Deref (not DerefMut), so we can't call recv() through it
                    let (chan, guard) = receiver.into_parts();
                    let mut fetched = 0;

                    // Timeout per block: 10 seconds. If no block arrives within this
                    // window, we give up and report partial success so the retry
                    // mechanism can try again with fresh provider info.
                    let per_block_timeout = std::time::Duration::from_secs(10);

                    loop {
                        match tokio::time::timeout(per_block_timeout, chan.recv()).await {
                            Ok(Ok(block)) => {
                                fetched += 1;
                                let block_cid = *block.cid();
                                let block_data = block.data().to_vec();

                                // Send block to coordinator for storage
                                if let Err(e) = event_tx
                                    .send(HostEvent::BitswapBlockReceived {
                                        query_id,
                                        cid: block_cid,
                                        data: block_data,
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
                            Ok(Err(_)) => {
                                // Channel closed — no more blocks coming
                                tracing::debug!("Bitswap channel closed, no more blocks");
                                break;
                            }
                            Err(_) => {
                                // Timeout — no block arrived within the window
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
                    info!(
                        query_id = query_id.0,
                        fetched = fetched,
                        total = missing_cids.len(),
                        success = success,
                        "Bitswap fetch complete"
                    );
                    // The guard closes the get_blocks loop, which is the only
                    // other holder of the session's channel; stop needs it gone.
                    drop(chan);
                    drop(guard);
                    if let Err(e) = session.stop().await {
                        warn!(query_id = query_id.0, error = %e, "Failed to stop Bitswap session");
                    }

                    // Notify completion
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
                }
                Err(e) => {
                    tracing::error!(
                        query_id = query_id.0,
                        error = %e,
                        "Bitswap get_blocks failed"
                    );
                    if let Err(e) = session.stop().await {
                        warn!(query_id = query_id.0, error = %e, "Failed to stop Bitswap session");
                    }
                    let _ = event_tx
                        .send(HostEvent::BitswapComplete {
                            query_id,
                            success: false,
                            error: Some(e.to_string()),
                        })
                        .await;
                }
            }
            queries.lock().remove(&query_id);
        });

        #[cfg(feature = "test-utils")]
        crate::testutil::stall_before_query_registration().await;

        // Store the join handle for cancellation support
        self.bitswap_queries
            .lock()
            .insert(query_id, (task_handle, session_id));
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
        let cancelled = if let Some((task_handle, session_id)) =
            self.bitswap_queries.lock().remove(&query_id)
        {
            debug!(query_id = ?query_id, "Cancelling Bitswap query");
            task_handle.abort();
            let client = self.swarm.behaviour().bitswap.client().clone();
            tokio::spawn(async move {
                // `abort` only schedules cancellation, and `Session::stop`
                // refuses to run while any other handle to the session is
                // alive. Joining the aborted task is what drops its clone;
                // it resolves with a cancelled `JoinError` rather than
                // hanging.
                let _ = task_handle.await;
                if let Err(e) = client.stop_session(session_id).await {
                    warn!(query_id = ?query_id, error = %e, "Failed to stop cancelled Bitswap session");
                }
            });
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
