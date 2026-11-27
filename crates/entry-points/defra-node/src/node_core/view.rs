//! Views as the caller `context` names, in the transaction it carries if it
//! carries one.

use defra_core::RequestContext;
use schema::CollectionVersion;

use super::NodeCore;
use crate::Result;

impl NodeCore<'_> {
    /// Create the views `target_sdl` defines over `source_query`, their
    /// documents run through the stored lens `transform` when it names one;
    /// the view collections created (Go: `client.Store::AddView`).
    pub(crate) async fn add_view(
        &self,
        context: &RequestContext,
        source_query: &str,
        target_sdl: &str,
        transform: Option<&str>,
    ) -> Result<Vec<CollectionVersion>> {
        self.node
            .ops()
            .add_view(
                &self.node.admitted_caller(context)?,
                source_query,
                target_sdl,
                transform,
            )
            .await
    }

    /// Refresh the materialized views `selector` names, every one when it
    /// names none (Go: `client.Store::RefreshViews`).
    pub(crate) async fn refresh_views(
        &self,
        context: &RequestContext,
        selector: db::CollectionSelector,
    ) -> Result<()> {
        self.node
            .ops()
            .refresh_views(&self.node.admitted_caller(context)?, selector)
            .await
    }
}
