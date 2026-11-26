//! Reading index entries without the documents they point to.

use std::sync::Arc;

use datastore::NamespaceView;
use document::NormalValue;
use lens::TransformStore;
use query::fetcher::IndexEntryValues;
use query::QueryError;
use storage::index::IndexIterator;

use crate::collection::Collection;
use crate::index::IndexManager;

/// The entries of `index_name` under each of `prefixes`, with their indexed
/// values, all read from one index handle.
/// `None` when a reindex is owed, so the caller reads the documents instead.
pub(crate) async fn prefix_entries(
    index_manager: &IndexManager,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    lens_store: Option<&Arc<dyn TransformStore>>,
    collection: &Collection,
    index_name: &str,
    prefixes: &[Vec<NormalValue>],
) -> query::error::Result<Option<Vec<Vec<IndexEntryValues>>>> {
    let collection_name = &collection.schema().name;
    let owed =
        crate::definition::migration::reindex_owed(systemstore, &collection.schema().collection_id)
            .await
            .map_err(|e| QueryError::execution(format!("owed reindex read: {e}")))?;
    if owed {
        return Ok(None);
    }
    let index = index_manager
        .read_index(datastore, systemstore, lens_store, collection, index_name)
        .await
        .map_err(|e| QueryError::execution(e.to_string()))?
        .ok_or_else(|| {
            QueryError::execution(format!(
                "index '{index_name}' not found on collection '{collection_name}'"
            ))
        })?;
    let mut found = Vec::with_capacity(prefixes.len());
    for prefix in prefixes {
        let entries = index
            .scan_prefix(datastore, prefix, false)
            .await
            .map_err(|e| QueryError::execution(format!("index error: {e}")))?
            .collect_all()
            .await
            .map_err(|e| QueryError::execution(format!("index iteration error: {e}")))?;
        found.push(
            entries
                .into_iter()
                .map(|entry| IndexEntryValues {
                    doc_short_id: entry.doc_short_id,
                    values: entry.values,
                })
                .collect(),
        );
    }
    Ok(Some(found))
}
