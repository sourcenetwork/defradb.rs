use async_trait::async_trait;
use datastore::NamespaceView;
use document::Document;
use query::doc_stream::DocStream;
use query::error::{QueryError, Result};
use query::planner::index_selection::{IndexScanParams, IndexScanType};
use rapidhash::{HashSetExt, RapidHashSet};
use storage::index::{IndexIterator, RangeIterator};

use crate::collection::Collection;
use crate::index::IndexManager;

pub(crate) struct IndexDocStream {
    collection: Collection,
    datastore: NamespaceView,
    systemstore: NamespaceView,
    iter: RangeIterator,
    params: IndexScanParams,
    seen: RapidHashSet<u64>,
    show_deleted: bool,
    fetched: u64,
    exhausted: bool,
}

impl IndexDocStream {
    pub(crate) fn supports(params: &IndexScanParams) -> bool {
        params.limit.is_none()
            && params.offset == 0
            && matches!(
                params.scan_type,
                IndexScanType::PrefixScan { .. } | IndexScanType::RangeScan { .. }
            )
    }

    pub(crate) async fn open(
        collection: Collection,
        datastore: NamespaceView,
        systemstore: NamespaceView,
        params: &IndexScanParams,
        show_deleted: bool,
    ) -> Result<Self> {
        let manager = IndexManager::from_indexes(
            collection.resolved_root_id(),
            collection.schema(),
            collection.write_indexes(),
            collection.crdts(),
        )
        .map_err(|e| QueryError::execution(e.to_string()))?;
        let index = manager
            .read_index(&datastore, &collection, &params.index_name)
            .await
            .map_err(|e| QueryError::execution(e.to_string()))?
            .ok_or_else(|| {
                QueryError::execution(format!("index '{}' not found", params.index_name))
            })?;
        let mut iter = match &params.scan_type {
            IndexScanType::PrefixScan {
                prefix_values,
                reverse,
            } => index.scan_prefix(&datastore, prefix_values, *reverse).await,
            IndexScanType::RangeScan {
                prefix_values,
                lower,
                upper,
                reverse,
            } => {
                index
                    .scan_range(
                        &datastore,
                        prefix_values,
                        lower.clone(),
                        upper.clone(),
                        *reverse,
                    )
                    .await
            }
            _ => return Err(QueryError::execution("unsupported streaming index scan")),
        }
        .map_err(|e| QueryError::execution(e.to_string()))?;
        crate::read::seek::apply_cursor_seek_to_iterator(
            &mut iter,
            &params.cursor_seek,
            &systemstore,
            collection.resolved_root_id(),
        )
        .await?;
        Ok(Self {
            collection,
            datastore,
            systemstore,
            iter,
            params: params.clone(),
            seen: RapidHashSet::new(),
            show_deleted,
            fetched: 0,
            exhausted: false,
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocStream for IndexDocStream {
    async fn next(&mut self) -> Result<Option<(Document, bool)>> {
        if self.exhausted {
            return Ok(None);
        }
        while let Some(entry) = self
            .iter
            .next()
            .await
            .map_err(|e| QueryError::execution(e.to_string()))?
        {
            self.fetched += 1;
            if let Some(filter) = &self.params.value_filter {
                if let Some(value) = entry.values.first() {
                    if !filter.matches_value(value) {
                        continue;
                    }
                }
            }
            if !self.seen.insert(entry.doc_short_id) {
                continue;
            }
            let docs = self
                .collection
                .get_by_short_ids(
                    &self.datastore,
                    &self.systemstore,
                    &[entry.doc_short_id],
                    self.show_deleted,
                )
                .await
                .map_err(|e| QueryError::execution(e.to_string()))?;
            if let Some((_, doc, deleted)) = docs.into_iter().next() {
                return Ok(Some((doc, deleted)));
            }
        }
        self.exhausted = true;
        Ok(None)
    }

    fn index_fetches(&self) -> u64 {
        self.fetched
    }

    async fn close(&mut self) -> Result<()> {
        self.exhausted = true;
        self.iter
            .close()
            .await
            .map_err(|e| QueryError::execution(e.to_string()))
    }
}
