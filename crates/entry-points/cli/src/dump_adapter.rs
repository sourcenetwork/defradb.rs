use std::sync::Arc;

use async_trait::async_trait;

use defra_http::router::DumpOperations;
use storage::corekv::Store;

/// HTTP's DumpOperations over the core's store dump.
pub struct DumpAdapter<S>
where
    S: Store,
{
    database: Arc<db::DB<S>>,
}

impl<S> DumpAdapter<S>
where
    S: Store + 'static,
{
    /// Create an Arc-wrapped adapter.
    pub fn new_arc(database: Arc<db::DB<S>>) -> Arc<dyn DumpOperations> {
        Arc::new(Self { database })
    }
}

#[async_trait]
impl<S> DumpOperations for DumpAdapter<S>
where
    S: Store + 'static,
{
    async fn print_dump(&self) -> Result<Vec<String>, String> {
        self.database
            .print_dump()
            .await
            .map_err(|error| error.to_string())
    }
}
