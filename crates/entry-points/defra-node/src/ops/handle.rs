//! The handles a node operation runs on, cloned out of [`Node`]: the node's
//! own calls and its HTTP server's router traits run the same operations.

use std::sync::Arc;

use query::QueryExecutor;

use crate::acp_ops::AcpOps;
use crate::{Error, Node, Result, SchemaOps};

pub(crate) type Database = Arc<db::DB<storage::RegolithStore>>;

/// What an operation reads and writes, and where it runs.
#[derive(Clone)]
pub(crate) struct NodeOps {
    pub(crate) database: Database,
    pub(crate) txn_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
    pub(crate) runner: Arc<dyn QueryExecutor>,
    pub(crate) schema_ops: Arc<dyn SchemaOps>,
    pub(crate) acp_ops: Arc<dyn AcpOps>,
    pub(crate) document_acp: Arc<dyn acp::DocumentACP>,
    pool: Option<workers::Pool>,
    remote_acp: bool,
}

impl NodeOps {
    /// Runs `make` on the database's worker pool.
    pub(crate) async fn on_pool<F, Fut, T>(&self, make: F) -> Result<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        workers::run_on(self.pool.as_ref(), workers::Target::LeastLoaded, make)
            .await
            .map_err(Error::WorkerPoolCall)?
    }

    /// Runs a document ACP call: on the pool when the ACP is the node's own
    /// store, where it is made when the ACP is a remote provider.
    pub(crate) async fn on_acp<F, Fut, T>(&self, make: F) -> Result<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        if self.remote_acp {
            make().await
        } else {
            self.on_pool(make).await
        }
    }
}

impl Node {
    /// The handles the node's operations run on.
    pub(crate) fn ops(&self) -> NodeOps {
        NodeOps {
            database: Arc::clone(&self.database),
            txn_registry: Arc::clone(&self.txn_registry),
            runner: Arc::clone(&self.runner),
            schema_ops: Arc::clone(&self.schema_ops),
            acp_ops: Arc::clone(&self.acp_ops),
            document_acp: Arc::clone(&self.document_acp),
            pool: self.pool.clone(),
            remote_acp: self.remote_acp,
        }
    }
}
