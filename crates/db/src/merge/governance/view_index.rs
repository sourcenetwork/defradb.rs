use std::sync::Arc;

use document::{Document, NormalValue};
use schema::IndexDescription;
use storage::corekv::Store;
use storage::index::{IndexIterator, SimpleIndex};

use super::view::DbMergeView;
use crate::collection::loader::load_collection_from_systemstore;
use crate::collection::Collection;

impl<S: Store, B: blockstore::Blockstore> DbMergeView<'_, S, B> {
    /// The documents `field`'s secondary index lists under `value` on this
    /// verdict's snapshot, or `None` when no index can stand in for a scan.
    ///
    /// A merge and a local write each put a document's index entries in the
    /// transaction that stores the document, so on one snapshot the index
    /// lists exactly the undeleted documents a scan would match; the deleted
    /// ones, whose entries a delete removed, are added from the deletion
    /// markers. A unique index is not used: a merge keeps one entry per
    /// value, where a scan sees every document holding it.
    pub(super) async fn indexed_documents(
        &self,
        collection: &Collection,
        field: &str,
        value: &NormalValue,
    ) -> Result<Option<Vec<Document>>, String> {
        let snapshot = self.snapshot().await?;
        let txn = snapshot.as_ref().expect("snapshot opened above");
        let datastore = txn.datastore().map_err(|error| error.to_string())?;
        let systemstore = txn.systemstore().map_err(|error| error.to_string())?;

        let key = (collection.name().to_string(), field.to_string());
        let chosen = self.indexes.lock().await.get(&key).cloned();
        let index = match chosen {
            Some(index) => index,
            None => {
                // The definition as the snapshot holds it: an index defined or
                // still backfilling after the snapshot opened is incomplete on it.
                let index = load_collection_from_systemstore(&systemstore, collection.name())
                    .await
                    .map_err(|error| error.to_string())?
                    .and_then(|stored| {
                        let index = stored
                            .queryable_indexes()
                            .iter()
                            .find(|i| serves(i, field))?;
                        SimpleIndex::try_new(stored.resolved_root_id(), index.clone()).ok()
                    })
                    .map(Arc::new);
                self.indexes.lock().await.insert(key, index.clone());
                index
            }
        };
        let Some(index) = index else {
            return Ok(None);
        };

        let mut entries = index
            .get(&datastore, std::slice::from_ref(value))
            .await
            .map_err(|error| error.to_string())?;
        let mut doc_short_ids = Vec::new();
        while let Some(entry) = entries.next().await.map_err(|error| error.to_string())? {
            doc_short_ids.push(entry.doc_short_id);
        }
        entries.close().await.map_err(|error| error.to_string())?;
        doc_short_ids.sort_unstable();

        let mut documents: Vec<Document> = collection
            .get_by_short_ids(&datastore, &systemstore, &doc_short_ids, true)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(_, document, _)| document)
            .collect();
        drop(snapshot);
        // A delete removes a document's index entries, and a governance read
        // must still see the document (`MergeView::find_documents`). The
        // caller filters by value, so the deleted rows join the candidates.
        documents.extend(self.deleted_documents(collection).await?);
        Ok(Some(documents))
    }
}

fn serves(index: &IndexDescription, field: &str) -> bool {
    !index.is_vector()
        && !index.resolved_unique()
        && matches!(index.fields.as_slice(), [only] if only.name == field)
}
