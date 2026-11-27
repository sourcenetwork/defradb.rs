//! Index reads of LensedAutoCommitFetcher that return counts and entries, not documents.

use storage::corekv::Store;

use crate::index::IndexManager;
use query::planner::index_selection::IndexScanParams;

use super::LensedAutoCommitFetcher;

impl<S: Store + 'static> LensedAutoCommitFetcher<S> {
    /// Count the index entries `params` would visit, stopping at `cap`.
    pub(super) async fn estimate_index_scan_inner(
        &self,
        collection_name: &str,
        params: &IndexScanParams,
        cap: u64,
    ) -> query::error::Result<Option<u64>> {
        let collection = self
            .db
            .get_collection(collection_name)
            .map_err(|e| query::error::QueryError::execution(format!("db error: {}", e)))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;
        let index_manager = IndexManager::from_indexes(
            collection.resolved_root_id(),
            collection.schema(),
            collection.write_indexes(),
            collection.crdts(),
        )
        .map_err(|e| query::error::QueryError::execution(format!("index manager error: {}", e)))?;
        let txn = self.db.new_txn(true).await.map_err(|e| {
            query::error::QueryError::execution(format!("failed to create txn: {}", e))
        })?;
        let datastore = txn.datastore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get datastore: {}", e))
        })?;
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get systemstore: {}", e))
        })?;
        let count = match index_manager
            .read_index(
                &datastore,
                &systemstore,
                Some(self.db.lens_store()),
                &collection,
                &params.index_name,
            )
            .await
        {
            Ok(Some(index)) => crate::read::index_count::count_index_scan(
                &index,
                &datastore,
                &params.scan_type,
                usize::try_from(cap).unwrap_or(usize::MAX),
            )
            .await
            .map(Some)
            .map_err(|e| e.to_string()),
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        };
        let _ = txn.discard();
        let count = count.map_err(|e| {
            query::error::QueryError::execution(format!("index count error: {}", e))
        })?;
        Ok(count.map(|count| count as u64))
    }

    pub(super) async fn index_prefix_entries_impl(
        &self,
        collection_name: &str,
        index_name: &str,
        prefixes: &[Vec<document::NormalValue>],
    ) -> query::error::Result<Option<Vec<Vec<query::fetcher::IndexEntryValues>>>> {
        let collection = self
            .db
            .get_collection(collection_name)
            .map_err(|e| query::error::QueryError::execution(format!("db error: {e}")))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;
        let index_manager = IndexManager::from_indexes(
            collection.resolved_root_id(),
            collection.schema(),
            collection.write_indexes(),
            collection.crdts(),
        )
        .map_err(|e| query::error::QueryError::execution(format!("index manager error: {e}")))?;
        let txn = self.db.new_txn(true).await.map_err(|e| {
            query::error::QueryError::execution(format!("failed to create txn: {e}"))
        })?;
        let datastore = txn.datastore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get datastore: {e}"))
        })?;
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get systemstore: {e}"))
        })?;
        let entries = crate::read::index_entries::prefix_entries(
            &index_manager,
            &datastore,
            &systemstore,
            Some(self.db.lens_store()),
            &collection,
            index_name,
            prefixes,
        )
        .await;
        let _ = txn.discard();
        entries
    }
}
