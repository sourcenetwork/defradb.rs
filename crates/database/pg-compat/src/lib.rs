mod bridge;
mod encode;
pub mod error;
mod handler;
mod metadata;
mod types;
pub mod wire;

use std::net::SocketAddr;
use std::pin::pin;
use std::sync::Arc;

use edge::{Edge, TaskSet};
use nuclei::net::TcpListener;
use query::{CollectionProvider, QueryExecutor};
use tracing::{error, info};

use handler::{DefraHandlerFactory, DefraQueryHandler};

pub use handler::SchemaManager;

/// Postgres wire protocol compatibility server for DefraDB.
///
/// Accepts psql connections and translates SQL queries to GraphQL,
/// executing them through the existing query engine.
pub struct PgServer {
    address: SocketAddr,
    factory: Arc<DefraHandlerFactory>,
    edge: Edge,
}

impl PgServer {
    /// A server whose connections run on `edge`.
    pub fn new(
        address: SocketAddr,
        executor: Arc<dyn QueryExecutor>,
        collections: Arc<dyn CollectionProvider>,
        schema_manager: Option<Arc<dyn SchemaManager>>,
        edge: Edge,
    ) -> Self {
        let handler = Arc::new(DefraQueryHandler::new(
            executor,
            collections,
            schema_manager,
        ));
        let factory = Arc::new(DefraHandlerFactory::new(handler, address.to_string()));
        Self {
            address,
            factory,
            edge,
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Runs the PG wire protocol server, accepting connections until its
    /// edge stops accepting. Each connection finishes the message in
    /// progress and closes, an idle one at once, and the server returns once
    /// they have.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.address).await?;
        info!(
            "Postgres wire protocol server listening on {}",
            self.address
        );

        let mut connections = TaskSet::new(self.edge.clone());
        let mut closing = pin!(self.edge.closing());
        loop {
            let accepted = nuclei::select! {
                accepted = listener.accept() => accepted,
                Some(_) = connections.join_next() => continue,
                () = closing.as_mut() => break,
            };
            let (socket, peer) = accepted?;
            info!("PG connection from {}", peer);

            let factory = self.factory.clone();
            let edge = self.edge.clone();
            connections.spawn(async move {
                if let Err(e) = wire::serve(socket, peer, factory, edge).await {
                    error!("PG connection error from {}: {}", peer, e);
                }
            });
        }
        while connections.join_next().await.is_some() {}
        Ok(())
    }
}
