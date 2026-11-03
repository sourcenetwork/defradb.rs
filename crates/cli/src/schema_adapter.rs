//! Adapter to bridge database schema operations to HTTP and PG wire protocol.

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::CollectionOperations;
use schema::CollectionVersion;
use storage::corekv::Store;

/// Adapter that implements schema operations using the database.
pub struct SchemaAdapter<S: Store> {
    database: Arc<db::DB<S>>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S: Store + 'static> SchemaAdapter<S> {
    /// Create a new adapter wrapping the given database.
    pub fn new(database: Arc<db::DB<S>>) -> Self {
        Self {
            database,
            context: None,
        }
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }

    /// Create an Arc-wrapped adapter for HTTP schema operations.
    pub fn new_arc(database: Arc<db::DB<S>>) -> Arc<dyn CollectionOperations> {
        Arc::new(Self::new(database))
    }

    /// Create an Arc-wrapped adapter for PG wire protocol schema operations.
    ///
    /// Use `new_pg_arc_with_acp` instead when the caller has an
    /// `AcpOperations` handle — that variant enables schema-time DRI
    /// validation (#746). This bare constructor exists for tests and
    /// for embedded PG callers that don't have ACP configured.
    #[cfg(feature = "postgres")]
    pub fn new_pg_arc(database: Arc<db::DB<S>>) -> Arc<dyn pg_compat::SchemaManager> {
        Arc::new(Self::new(database))
    }

    async fn add_collection_inner(&self, sdl: &str) -> Result<Vec<CollectionVersion>, String> {
        self.database
            .add_collection(&self.context(), sdl)
            .await
            .map_err(|e| e.to_string())
    }
}

#[async_trait]
impl<S: Store + 'static> CollectionOperations for SchemaAdapter<S> {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn CollectionOperations>> {
        Some(Arc::new(Self {
            database: Arc::clone(&self.database),
            context: Some(context.clone()),
        }))
    }

    async fn add_collection(&self, sdl: &str) -> Result<Vec<CollectionVersion>, String> {
        self.add_collection_inner(sdl).await
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl<S: Store + 'static> pg_compat::SchemaManager for SchemaAdapter<S> {
    async fn add_collection(&self, sdl: &str) -> Result<(), String> {
        self.add_collection_inner(sdl).await?;
        Ok(())
    }
}
