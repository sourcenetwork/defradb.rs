//! Views as the caller `context` names, on the database's worker pool.

use defra_core::RequestContext;
use schema::CollectionVersion;

use super::handle::NodeOps;
use crate::Result;

impl NodeOps {
    /// [`crate::Node::add_view_as`].
    pub(crate) async fn add_view(
        &self,
        context: &RequestContext,
        source_query: &str,
        target_sdl: &str,
        transform: Option<&str>,
    ) -> Result<Vec<CollectionVersion>> {
        let (ops, context) = (self.schema_ops.clone(), context.clone());
        let (query, sdl) = (source_query.to_owned(), target_sdl.to_owned());
        let transform = transform.map(str::to_owned);
        self.on_pool(move || async move {
            ops.add_view(&context, &query, &sdl, transform.as_deref())
                .await
        })
        .await
    }

    /// [`crate::Node::refresh_views_as`].
    pub(crate) async fn refresh_views(
        &self,
        context: &RequestContext,
        selector: db::CollectionSelector,
    ) -> Result<()> {
        let (ops, context) = (self.schema_ops.clone(), context.clone());
        self.on_pool(move || async move { ops.refresh_views(&context, selector).await })
            .await
    }
}
