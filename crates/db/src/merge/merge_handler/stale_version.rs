//! A merged document stored at a version other than the active one.
//!
//! Queries read a document through the lenses between its stored version and
//! the active one, but a merge indexes the values it stores. When those
//! versions differ across a lens, the entries may not describe what queries
//! read, so the merge owes the collection a reindex in its own transaction:
//! until the rebuild settles, no index answers queries and they scan; the
//! rebuild migrates the document to the active version and indexes it again.
//! The merge does not run the rebuild: like any owed reindex it settles after
//! the next schema change and at open.

use super::*;
use crate::definition::lens::build_collection_history;
use crate::definition::loader::get_collections_by_collection_id;
use crate::definition::migration::owe_reindex;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Owes `collection`'s collection a reindex, in the merge's transaction,
    /// when the merge stored a document at `collection`'s version and the
    /// active version reads it through a lens. It owes a full rebuild every
    /// time, so a rebuild already running when the document arrived does not
    /// clear the record without indexing it.
    pub(super) async fn owe_reindex_off_active_version(
        &self,
        systemstore: &NamespaceView,
        collection: &Collection,
    ) -> std::result::Result<(), MergeError> {
        let Some(active) = self
            .db
            .get_collection(collection.name())
            .map_err(MergeError::Database)?
        else {
            return Ok(());
        };
        if active.version_id() == collection.version_id() || active.get_indexes().is_empty() {
            return Ok(());
        }
        let collection_id = collection.collection_id();
        let versions = get_collections_by_collection_id(systemstore, collection_id)
            .await
            .map_err(MergeError::Database)?;
        let across_lens = build_collection_history(&versions, active.version_id())
            .is_some_and(|history| history.values().any(|link| link.transform.is_some()));
        if !across_lens {
            return Ok(());
        }
        owe_reindex(systemstore, &self.db.collections, collection_id, None)
            .await
            .map_err(MergeError::Database)
    }
}
