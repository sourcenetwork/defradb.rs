//! Refreshing materialized views and collecting downsample histories.

use defra_core::RequestContext;
use storage::corekv::Store;

use super::{Result, ViewError};
use crate::downsample::GcDownsampleHistoriesOptions;
use crate::CollectionSelector;
use crate::DbTransactionRegistry;

impl<S> DbTransactionRegistry<S>
where
    S: Store + 'static,
{
    /// Go's `RefreshViews`: rebuild the materialized views `selector` names,
    /// every one when it names none, in the transaction `context` carries if
    /// it carries one.
    pub async fn refresh_views_as(
        &self,
        context: &RequestContext,
        selector: CollectionSelector,
    ) -> Result<()> {
        let db = self.db();
        db.check_node_access_as(context, acp::nac::NodePermission::ViewRefresh)
            .await?;
        match context.txn() {
            Some(txn) => self
                .refresh_views_in_txn(context, &txn.to_string(), selector)
                .await
                .map_err(ViewError::Refresh),
            None => db.refresh_views(selector).await.map_err(ViewError::Refresh),
        }
    }

    /// Drop the downsample histories past their retention for the views
    /// `names` lists, every downsample view when it lists none.
    pub async fn gc_downsample_views_as(
        &self,
        context: &RequestContext,
        names: Option<Vec<String>>,
    ) -> Result<()> {
        let db = self.db();
        db.check_node_access_as(context, acp::nac::NodePermission::ViewGc)
            .await?;
        let options = names.map(GcDownsampleHistoriesOptions::with_names);
        Ok(db.gc_downsample_histories_as(context, options).await?)
    }
}
