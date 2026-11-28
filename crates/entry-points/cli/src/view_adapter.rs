//! HTTP's ViewOperations over the core's view operations. A request in a
//! transaction runs in it.

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::ViewOperations;
use schema::CollectionVersion;
use storage::corekv::Store;

/// The core's view operations, serving one request once bound to it.
pub struct ViewAdapter<S>
where
    S: Store + 'static,
{
    registry: Arc<db::DbTransactionRegistry<S>>,
    query_limits: query::QueryLimits,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S> ViewAdapter<S>
where
    S: Store + 'static,
{
    /// An adapter parsing view queries within `query_limits`.
    pub fn new_arc(
        registry: Arc<db::DbTransactionRegistry<S>>,
        query_limits: query::QueryLimits,
    ) -> Arc<dyn ViewOperations> {
        Arc::new(Self {
            registry,
            query_limits,
            context: None,
        })
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }
}

fn text(error: db::ViewError) -> String {
    error.to_string()
}

#[async_trait]
impl<S> ViewOperations for ViewAdapter<S>
where
    S: Store + 'static,
{
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn ViewOperations>> {
        Some(Arc::new(Self {
            registry: Arc::clone(&self.registry),
            query_limits: self.query_limits,
            context: Some(context.clone()),
        }))
    }

    async fn add_view(
        &self,
        gql_query: &str,
        sdl: &str,
        transform: Option<&str>,
    ) -> Result<Vec<CollectionVersion>, String> {
        self.registry
            .add_view(
                &self.context(),
                gql_query,
                sdl,
                transform,
                self.query_limits,
            )
            .await
            .map_err(text)
    }

    async fn refresh_views(&self, options: db::CollectionSelector) -> Result<(), String> {
        self.registry
            .refresh_views_as(&self.context(), options)
            .await
            .map_err(text)
    }

    async fn gc_downsample_histories(&self, names: Option<Vec<String>>) -> Result<(), String> {
        self.registry
            .gc_downsample_views_as(&self.context(), names)
            .await
            .map_err(text)
    }
}
