//! Node run loop and shutdown coordination

use std::time::Duration;

use nuclei::time::timeout;
use tracing::{error, info, warn};

use super::node::Node;
use crate::error::{Error, Result};

/// How long shutdown waits for the HTTP requests in flight, as long as the
/// network runtime's own drain.
const HTTP_DRAIN: Duration = Duration::from_secs(30);

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

        // Fallible setup comes before the servers start, so an error leaves
        // nothing running.
        #[cfg(unix)]
        let (mut sigint, mut sigterm) = {
            use nuclei::signal::unix::{signal, SignalKind};
            (
                signal(SignalKind::interrupt()).map_err(|e| Error::Signal(e.to_string()))?,
                signal(SignalKind::terminate()).map_err(|e| Error::Signal(e.to_string()))?,
            )
        };

        // Start HTTP server, in its own scope. Its task is awaited below to
        // notice a crash, so it carries no deadline.
        let mut http_task: Option<edge::Task<()>> = if let Some(server) = self.http_server.take() {
            info!("Starting HTTP server on {}", self.config.api.address);
            Some(
                self.http_scope
                    .edge()
                    .spawn(async move {
                        if let Err(e) = server.run().await {
                            error!("HTTP server error: {}", e);
                        }
                    })
                    .within(Duration::MAX),
            )
        } else {
            None
        };

        // Start PG wire protocol server
        #[cfg(not(feature = "postgres"))]
        let pg_task: Option<edge::Service> = None;
        #[cfg(feature = "postgres")]
        let pg_task: Option<edge::Service> = if let Some(server) = self.pg_server.take() {
            info!(
                "Starting Postgres wire protocol server on {}",
                server.address()
            );
            Some(self.edge.spawn_service(async move {
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
            self.edge
                .spawn_service(async move {
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
            self.edge.spawn_service(async move {
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
        let http_crashed = match &mut http_task {
            Some(task) => {
                nuclei::select! {
                    _ = self.shutdown_rx.recv() => false,
                    result = task => {
                        match result {
                            Ok(()) => {
                                error!("HTTP server exited unexpectedly");
                            }
                            Err(edge::EdgeError::Panicked(e)) => {
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
            // Only drain if we're shutting down normally (not due to crash)
            if let Some(task) = http_task {
                info!("Stopping HTTP server...");
                let drained = self.http_scope.drain(HTTP_DRAIN).await;
                if drained.is_complete() {
                    info!("HTTP server stopped");
                } else {
                    task.abort();
                    warn!(
                        remaining = drained.remaining,
                        timeout_secs = HTTP_DRAIN.as_secs(),
                        "HTTP requests still running at the shutdown deadline were stopped"
                    );
                }
            }
            if let Some(task) = pg_task {
                info!("Stopping PG server...");
                let _ = timeout(std::time::Duration::from_secs(1), task.stopped()).await;
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
                    replication_task.stop();
                    host_task.stop();
                    failure_recorder_task.abort();
                    retry_loop_task.abort();
                    if let Some(event_task) = event_handler_task {
                        event_task.stop();
                    }

                    let _ = timeout(grace, replication_task.stopped()).await;
                    let _ = timeout(grace, host_task.stopped()).await;
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
