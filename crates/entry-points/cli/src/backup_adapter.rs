use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::{BackupOperations, ImportResult};
use query::executor::QueryExecutor;
use storage::corekv::Store;

/// Adapter that implements BackupOperations using database.
pub struct BackupAdapter<S: Store> {
    database: Arc<db::DB<S>>,
    runner: Arc<dyn QueryExecutor>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S: Store + 'static> BackupAdapter<S> {
    /// Create an Arc-wrapped adapter.
    pub fn new_arc(
        database: Arc<db::DB<S>>,
        runner: Arc<dyn QueryExecutor>,
    ) -> Arc<dyn BackupOperations> {
        Arc::new(Self {
            database,
            runner,
            context: None,
        })
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }
}

#[async_trait]
impl<S: Store + 'static> BackupOperations for BackupAdapter<S> {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn BackupOperations>> {
        Some(Arc::new(Self {
            database: Arc::clone(&self.database),
            runner: Arc::clone(&self.runner),
            context: Some(context.clone()),
        }))
    }

    async fn export(
        &self,
        collections: Option<Vec<String>>,
        pretty: bool,
    ) -> Result<String, String> {
        let cols = collections.unwrap_or_default();
        db::backup::export_database_as(&self.context(), &self.database, &self.runner, &cols, pretty)
            .await
    }

    async fn import(&self, data: &str) -> Result<ImportResult, String> {
        let stats =
            db::backup::import_database_as(&self.context(), &self.database, &self.runner, data)
                .await?;

        Ok(ImportResult {
            documents_imported: stats.documents_imported,
            documents_skipped: 0,
            collections_affected: stats.collections_affected,
            errors: vec![],
        })
    }
}
