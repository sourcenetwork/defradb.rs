//! Node run loop and shutdown coordination

use nuclei::task::{AbortHandle, Join, Task};
use nuclei::time::timeout;
use tracing::{error, info, warn};

use super::node::Node;
use crate::error::{Error, Result};

impl Node {
    /// Run the node until shutdown
    #[doc(hidden)]
    pub async fn run(mut self) -> Result<()> {
        info!("DefraDB node started");
        let scheme = if self.config.api.tls_enabled() {
            "https"
        } else {
            "http"
        };
        info!("API endpoint: {scheme}://{}", self.config.api.address);

        // Start HTTP server
        let http_task: Option<Task<()>> = if let Some(server) = self.http_server.take() {
            info!("Starting HTTP server on {}", self.config.api.address);
            Some(nuclei::spawn(async move {
                if let Err(e) = server.run().await {
                    error!("HTTP server error: {}", e);
                }
            }))
        } else {
            None
        };

        // Start PG wire protocol server
        #[cfg(not(feature = "postgres"))]
        let pg_task: Option<Task<()>> = None;
        #[cfg(feature = "postgres")]
        let pg_task: Option<Task<()>> = if let Some(server) = self.pg_server.take() {
            info!(
                "Starting Postgres wire protocol server on {}",
                server.address()
            );
            Some(nuclei::spawn(async move {
                if let Err(e) = server.run().await {
                    error!("PG server error: {}", e);
                }
            }))
        } else {
            None
        };

        // Set up signal handling
        let shutdown_tx = self.shutdown_tx.clone();

        #[cfg(unix)]
        {
            use nuclei::signal::unix::{signal, SignalKind};

            let mut sigint =
                signal(SignalKind::interrupt()).map_err(|e| Error::Signal(e.to_string()))?;
            let mut sigterm =
                signal(SignalKind::terminate()).map_err(|e| Error::Signal(e.to_string()))?;

            nuclei::spawn(async move {
                nuclei::select! {
                    _ = sigint.recv() => {
                        info!("Received SIGINT");
                    }
                    _ = sigterm.recv() => {
                        info!("Received SIGTERM");
                    }
                }
                if let Err(e) = shutdown_tx.send(()).await {
                    error!("Failed to send shutdown signal: {}", e);
                }
            })
            .detach();
        }

        #[cfg(not(unix))]
        {
            nuclei::spawn(async move {
                match nuclei::signal::ctrl_c().await {
                    Ok(()) => {
                        info!("Received Ctrl+C");
                        if let Err(e) = shutdown_tx.send(()).await {
                            error!("Failed to send shutdown signal: {}", e);
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to listen for Ctrl+C signal: {}. Node may not respond to interrupt signals.",
                            e
                        );
                    }
                }
            })
            .detach();
        }

        // Wait for shutdown signal OR HTTP server crash
        let http_abort: Option<AbortHandle> = http_task.as_ref().map(Task::abort_handle);
        let mut http_join: Option<Join<()>> = http_task.map(Task::join);
        let http_crashed = match &mut http_join {
            Some(task) => {
                nuclei::select! {
                    _ = self.shutdown_rx.recv() => false,
                    result = task => {
                        match result {
                            Ok(()) => {
                                error!("HTTP server exited unexpectedly");
                            }
                            Err(e) if e.is_panic() => {
                                error!("HTTP server panicked: {}", e);
                            }
                            Err(e) => {
                                error!("HTTP server task failed: {}", e);
                            }
                        }
                        true
                    }
                }
            }
            None => {
                self.shutdown_rx.recv().await;
                false
            }
        };

        if http_crashed {
            info!("Initiating shutdown due to HTTP server failure...");
        } else {
            info!("Shutting down DefraDB node...");
            // Only abort if we're shutting down normally (not due to crash)
            if let (Some(abort), Some(task)) = (http_abort, http_join) {
                info!("Stopping HTTP server...");
                abort.abort();
                match timeout(std::time::Duration::from_secs(1), task).await {
                    Ok(_) => info!("HTTP server stopped"),
                    Err(_) => warn!(
                        timeout_secs = 1,
                        "HTTP server shutdown timed out - server was forcefully terminated. \
                         This may occur if requests were still in flight."
                    ),
                }
            }
            if let Some(task) = pg_task {
                info!("Stopping PG server...");
                task.abort();
                let _ = timeout(std::time::Duration::from_secs(1), task.join()).await;
                info!("PG server stopped");
            }
        }

        if let Some(task) = self.downsample_task.take() {
            info!("Stopping downsample worker...");
            task.abort();
            let _ = timeout(std::time::Duration::from_secs(1), task).await;
        }

        if let Some(task) = self.vector_rebuild_task.take() {
            info!("Stopping vector index rebuild worker...");
            task.abort();
            let _ = timeout(std::time::Duration::from_secs(1), task).await;
        }

        if let Some(task) = self.txn_cleanup_task.take() {
            info!("Stopping transaction idle cleanup worker...");
            task.abort();
            let _ = timeout(std::time::Duration::from_secs(1), task).await;
        }

        // Shutdown P2P tasks first, then the handle
        if let Some(tasks) = self.p2p_tasks.take() {
            info!("Stopping P2P background tasks...");
            let grace = std::time::Duration::from_secs(2);

            match tasks {
                super::node::P2PTasks::Libp2p {
                    coordinator,
                    host_task,
                    replication_task,
                    event_handler_task,
                    failure_recorder_task,
                    retry_loop_task,
                } => {
                    coordinator.shutdown().await;

                    // Abort all tasks - they will stop when the channel closes
                    replication_task.abort();
                    host_task.abort();
                    failure_recorder_task.abort();
                    retry_loop_task.abort();
                    if let Some(event_task) = event_handler_task {
                        event_task.abort();
                    }

                    let _ = timeout(grace, replication_task.join()).await;
                    let _ = timeout(grace, host_task.join()).await;
                }
                #[cfg(feature = "iroh")]
                super::node::P2PTasks::Iroh {
                    peer,
                    #[cfg(feature = "iroh-relay-server")]
                    relay_server,
                } => {
                    peer.shutdown().await;

                    #[cfg(feature = "iroh-relay-server")]
                    if let Some(relay_server) = relay_server {
                        match timeout(grace, relay_server.shutdown()).await {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => {
                                warn!("iroh relay server shutdown encountered an issue: {}", e)
                            }
                            Err(_) => warn!("iroh relay server did not stop within {:?}", grace),
                        }
                    }
                }
            }

            info!("P2P background tasks stopped");
        }

        if let Some(handle) = &self.p2p_handle {
            if let Err(e) = handle.shutdown().await {
                warn!("P2P shutdown encountered an issue: {}", e);
            }
        }

        // Nothing reaches the data path any more: the pool finishes what it
        // admitted and stops, then the store closes. The runtime is dropped
        // after this returns, outside any async context.
        info!("Stopping the worker pool and closing the database...");
        if let Err(e) = self.database.close().await {
            warn!("Database close encountered an issue: {}", e);
        }

        defra_core::signing::clear_identity_store();
        info!("DefraDB node shutdown complete");
        Ok(())
    }
}
