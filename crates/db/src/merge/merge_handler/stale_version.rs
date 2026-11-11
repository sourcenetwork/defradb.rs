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
use crate::definition::loader::{get_collection_by_version_id, get_collections_by_collection_id};
use crate::definition::migration::owe_reindex;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Owes `collection`'s collection a reindex, in the merge's transaction,
    /// when the active version `active_version` has indexes and reads
    /// documents stored at `collection`'s version through a lens. The active
    /// version is the one the merge's validated name read returned, and every
    /// definition is read in the same transaction, so a schema change that
    /// commits first aborts the merge and the re-run decides again. It owes a
    /// full rebuild every time, so a rebuild already running when the
    /// document arrived does not clear the record without indexing it.
    pub(super) async fn owe_reindex_off_active_version(
        &self,
        systemstore: &NamespaceView,
        collection: &Collection,
        active_version: &str,
    ) -> std::result::Result<(), MergeError> {
        let indexed = get_collection_by_version_id(systemstore, active_version)
            .await
            .map_err(MergeError::Database)?
            .is_some_and(|active| !active.indexes.is_empty());
        if !indexed {
            return Ok(());
        }
        let collection_id = collection.collection_id();
        let versions = get_collections_by_collection_id(systemstore, collection_id)
            .await
            .map_err(MergeError::Database)?;
        let across_lens = build_collection_history(&versions, active_version)
            .is_some_and(|history| history.values().any(|link| link.transform.is_some()));
        if !across_lens {
            return Ok(());
        }
        owe_reindex(systemstore, &self.db.collections, collection_id, None)
            .await
            .map_err(MergeError::Database)
    }
}
