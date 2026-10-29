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
    /// caller, ordered by name then version ID.
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

/// The versions of `versions` that `selector` picks, ordered by name then
/// version ID. Go propagates not-found only from its direct version lookup
/// (`internal/db/collection.go:210-215`): `version_id=nope` alone is an
/// error, while `name=Users&version_id=nope` is an empty answer.
pub(crate) fn select(
    versions: Vec<CollectionVersion>,
    selector: &CollectionSelector,
) -> Result<Vec<CollectionVersion>> {
    if let Some(version_id) = selector
        .version_id
        .as_ref()
        .filter(|_| selector.resolves_by_version_lookup())
    {
        if !versions.iter().any(|v| &v.version_id == version_id) {
            return Err(Error::CollectionVersionNotFound(version_id.clone()));
        }
    }
    let mut selected: Vec<_> = versions
        .into_iter()
        .filter(|version| selector.selects(version))
        .collect();
    selected.sort_by(|a, b| (&a.name, &a.version_id).cmp(&(&b.name, &b.version_id)));
    Ok(selected)
}
