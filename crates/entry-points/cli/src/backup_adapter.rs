use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::{BackupOperations, ImportResult};
use query::executor::QueryExecutor;
use storage::corekv::Store;

/// HTTP's BackupOperations over the core's backup export and import.
pub struct BackupAdapter<S>
where
    S: Store,
{
    database: Arc<db::DB<S>>,
    runner: Arc<dyn QueryExecutor>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S> BackupAdapter<S>
where
    S: Store + 'static,
{
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

fn text(error: db::backup::BackupError) -> String {
    error.to_string()
}

#[async_trait]
impl<S> BackupOperations for BackupAdapter<S>
where
    S: Store + 'static,
{
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
        let collections = collections.unwrap_or_default();
        db::backup::export_database_as(
            &self.context(),
            &self.database,
            &self.runner,
            &collections,
            pretty,
        )
        .await
        .map_err(text)
    }

    async fn import(
        &self,
        data: &str,
        host: &dyn db::backup::ImportHost,
    ) -> Result<ImportResult, String> {
        db::backup::import_database_as(&self.context(), &self.database, host, data)
            .await
            .map(ImportResult::from)
            .map_err(text)
    }
}
