mod bridge;
mod encode;
pub mod error;
mod handler;
mod metadata;
mod types;
pub mod wire;

use std::net::SocketAddr;
use std::sync::Arc;

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
}

impl PgServer {
    pub fn new(
        address: SocketAddr,
        executor: Arc<dyn QueryExecutor>,
        collections: Arc<dyn CollectionProvider>,
        schema_manager: Option<Arc<dyn SchemaManager>>,
    ) -> Self {
        let handler = Arc::new(DefraQueryHandler::new(
            executor,
            collections,
            schema_manager,
        ));
        let factory = Arc::new(DefraHandlerFactory::new(handler, address.to_string()));
        Self { address, factory }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Run the PG wire protocol server, accepting connections in a loop.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.address).await?;
        info!(
            "Postgres wire protocol server listening on {}",
            self.address
        );

        loop {
            let (socket, peer) = listener.accept().await?;
            info!("PG connection from {}", peer);

            let factory = self.factory.clone();
            nuclei::spawn(async move {
                if let Err(e) = wire::serve(socket, peer, factory).await {
                    error!("PG connection error from {}: {}", peer, e);
                }
            })
            .detach();
        }
    }
}
