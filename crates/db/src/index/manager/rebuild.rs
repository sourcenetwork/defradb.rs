//! Indexing a document again during a rebuild, after the entries were cleared.

use datastore::NamespaceView;
use document::Document;
use rapidhash::RapidHashMap;
use schema::CollectionVersion;

use super::{require_doc_short_id, IndexManager, IndexType};
use crate::index::error::{Error, Result};

impl IndexManager {
    /// Index `doc` in every index the way a rebuild does once the entries
    /// were cleared. A unique value another live document holds goes to the
    /// deterministic winner when both documents held it before the migration
    /// the rebuild applies, and is refused when that migration gave it to
    /// either of them. `original_keys` holds, for each document the rebuild
    /// migrated so far in this transaction, its unique keys from before; a
    /// document missing from it ran no transform.
    pub(crate) async fn reindex_document(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        doc_short_id: u64,
        doc: &Document,
        schema: &CollectionVersion,
        original_keys: &RapidHashMap<u64, rapidhash::RapidHashSet<Vec<u8>>>,
    ) -> Result<()> {
        require_doc_short_id(doc_short_id)?;
        let mut mutable_datastore = datastore.clone();
        for index in self.indexes.values() {
            let value_sets = self.extract_index_values(doc, index.description(), schema)?;
            if self.is_deferred(index.description(), schema) {
                self.place(datastore, index, doc_short_id, &value_sets)
                    .await?;
                continue;
            }
            let IndexType::Unique(unique) = index else {
                for values in &value_sets {
                    self.save_healing_stale_unique(
                        &mut mutable_datastore,
                        index,
                        doc_short_id,
                        values,
                    )
                    .await
                    .map_err(Error::Storage)?;
                }
                continue;
            };
            let doc_id = doc
                .id()
                .ok_or_else(|| Error::InvalidDocument("document must have an ID".to_string()))?
                .to_string();
            // Conflict resolution is a read-modify-write on the shared
            // transaction, so the values go in one at a time: the winner then
            // depends on the public DocID ordering, never on scheduling.
            for values in &value_sets {
                self.save_rebuilt_unique(
                    &mut mutable_datastore,
                    systemstore,
                    index,
                    unique,
                    doc_short_id,
                    &doc_id,
                    values,
                    original_keys,
                )
                .await?;
            }
        }
        Ok(())
    }
}
