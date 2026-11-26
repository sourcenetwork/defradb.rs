//! Reading index entries without the documents they point to.

use document::NormalValue;
use query::fetcher::IndexEntryValues;
use query::QueryError;
use storage::corekv::{MaybeSend, Reader};
use storage::index::IndexIterator;

use crate::index::IndexManager;

/// The entries of `index_name` under each of `prefixes`, with their indexed
/// values, all read from one index handle.
pub(crate) async fn prefix_entries<R>(
    index_manager: &IndexManager,
    datastore: &R,
    collection_name: &str,
    index_name: &str,
    prefixes: &[Vec<NormalValue>],
) -> query::error::Result<Vec<Vec<IndexEntryValues>>>
where
    R: Reader + MaybeSend,
{
    let index = index_manager.get_index(index_name).ok_or_else(|| {
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
    Ok(found)
}
