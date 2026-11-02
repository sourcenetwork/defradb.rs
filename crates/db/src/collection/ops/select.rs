//! Go's `GetCollections`: the collection versions a selector picks.

use schema::CollectionSelector;
use schema::CollectionVersion;
use storage::corekv::Store;

use crate::error::{Error, Result};

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// The committed collection versions `selector` picks for `context`'s
    /// caller, in Go's order (see [`CollectionSelector::select`]).
    pub async fn get_collections_as(
        &self,
        context: &defra_core::RequestContext,
        selector: &CollectionSelector,
    ) -> Result<Vec<CollectionVersion>> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionGet)
            .await?;
        // Inactive versions cost a scan of every stored version; a selector
        // that does not ask for them is answered from the active listing.
        let versions = match selector.needs_all_versions() {
            true => self.get_all_collection_versions().await?,
            false => self.get_active_collection_versions()?,
        };
        select(versions, selector)
    }
}

/// [`CollectionSelector::select`], a lone version lookup's miss as Go's
/// not-found error.
pub(crate) fn select(
    versions: Vec<CollectionVersion>,
    selector: &CollectionSelector,
) -> Result<Vec<CollectionVersion>> {
    selector.select(versions).ok_or_else(|| {
        Error::CollectionVersionNotFound(selector.version_id.clone().unwrap_or_default())
    })
}
