//! Go's collection lookup (`GetCollections`), shared by every call that
//! takes `CollectionOptions`.

use std::sync::Arc;

use schema::CollectionVersion;

/// What a lookup that must find one collection reports when it finds none.
pub(crate) const ERR_COLLECTION_NOT_FOUND: &str = "collection not found";

/// What a lookup that must find one collection reports when it finds several.
const ERR_AMBIGUOUS_COLLECTION: &str = "more than one collection matches the given criteria";

/// The collection versions `selector` selects, in the order Go's
/// `GetCollections` returns them, read in the transaction `context` names
/// or from the node's store.
pub(crate) async fn collections(
    runner: &Arc<dyn query::QueryExecutor>,
    context: &defra_core::RequestContext,
    selector: &db::CollectionSelector,
) -> Result<Vec<CollectionVersion>, String> {
    runner
        .get_collections_as(context, selector)
        .await
        .map_err(|e| e.to_string())
}

/// The one collection version `selector` selects, as Go's `getCollection`
/// requires.
pub(crate) async fn collection(
    runner: &Arc<dyn query::QueryExecutor>,
    context: &defra_core::RequestContext,
    selector: &db::CollectionSelector,
) -> Result<CollectionVersion, String> {
    let mut selected = collections(runner, context, selector).await?;
    match selected.len() {
        0 => Err(ERR_COLLECTION_NOT_FOUND.to_string()),
        1 => Ok(selected.remove(0)),
        _ => Err(ERR_AMBIGUOUS_COLLECTION.to_string()),
    }
}
