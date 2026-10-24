//! Streams transformed documents without writing to storage.

use rapidhash::RapidHashMap;

use async_trait::async_trait;
use document::Document;
use lens::TargetedHistoryLink;
use query::doc_stream::DocStream;
use storage::corekv::{IterOptions, Store};

use crate::collection::stream::CollectionDocStream;
use crate::collection::Collection;
use crate::read::autocommit::AutoCommitDocStream;
use crate::txn::DbTxn;

use super::LensedAutoCommitFetcher;

/// Pulls documents from storage and applies the auto-commit fetcher's
/// per-document migration to each one as it is yielded.
///
/// The source is an [`AutoCommitDocStream`], which owns the read-only
/// transaction auto-commit fetchers open per operation and releases it when
/// the source is exhausted, closed or dropped.
pub struct LensedAutoCommitDocStream<S: Store + 'static> {
    source: AutoCommitDocStream<S>,
    fetcher: LensedAutoCommitFetcher<S>,
    collection: Collection,
    has_migrations: bool,
    preloaded_history: Option<RapidHashMap<String, TargetedHistoryLink>>,
}

impl<S: Store + 'static> LensedAutoCommitDocStream<S> {
    /// Wrap an inner stream, optionally holding the read transaction it owns.
    pub fn new(
        inner: Box<dyn DocStream>,
        txn: Option<DbTxn<S>>,
        fetcher: LensedAutoCommitFetcher<S>,
        collection: Collection,
        has_migrations: bool,
        preloaded_history: Option<RapidHashMap<String, TargetedHistoryLink>>,
    ) -> Self {
        let source = match txn {
            Some(txn) => AutoCommitDocStream::new(inner, txn),
            None => AutoCommitDocStream::without_txn(inner),
        };
        Self {
            source,
            fetcher,
            collection,
            has_migrations,
            preloaded_history,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> DocStream for LensedAutoCommitDocStream<S> {
    async fn next(&mut self) -> query::error::Result<Option<(Document, bool)>> {
        let Some((doc, is_deleted)) = self.source.next().await? else {
            return Ok(None);
        };

        let processed = self
            .fetcher
            .process_document(
                doc,
                &self.collection,
                self.has_migrations,
                &self.preloaded_history,
            )
            .await?;
        Ok(Some((processed.document, is_deleted)))
    }

    fn index_fetches(&self) -> u64 {
        self.source.index_fetches()
    }

    /// Close the inner stream and release its read snapshot.
    async fn close(&mut self) -> query::error::Result<()> {
        self.source.close().await
    }
}

impl<S: Store + 'static> LensedAutoCommitFetcher<S> {
    pub(super) async fn stream_all_with_deleted_impl(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> query::error::Result<Box<dyn DocStream>> {
        let collection = self
            .db
            .get_collection(collection_name)
            .map_err(|e| query::error::QueryError::execution(format!("db error: {}", e)))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;

        let (_, has_migrations, preloaded_history) =
            self.load_migration_context(&collection).await?;

        let txn = self.db.new_txn(true).await.map_err(|e| {
            query::error::QueryError::execution(format!("failed to create txn: {}", e))
        })?;

        let datastore = txn.datastore().map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to get datastore for collection '{}': {}",
                collection_name, e
            ))
        })?;
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get systemstore: {}", e))
        })?;

        let prefix = collection.collection_key_prefix();
        let prefix_len = prefix.len();
        let opts = IterOptions::new().with_prefix(prefix);
        let iter = datastore
            .iterator(opts)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("storage error: {}", e)))?;

        let inner = CollectionDocStream::new(
            collection.clone(),
            datastore,
            systemstore,
            iter,
            prefix_len,
            show_deleted,
        );

        Ok(Box::new(LensedAutoCommitDocStream::new(
            Box::new(inner),
            Some(txn),
            self.clone_for_stream(),
            collection,
            has_migrations,
            preloaded_history,
        )))
    }
}

impl<S: Store + 'static> LensedAutoCommitFetcher<S> {
    /// The seeking counterpart of
    /// [`stream_all_with_deleted_impl`](Self::stream_all_with_deleted_impl):
    /// identical but for its source, point reads instead of a prefix scan.
    pub(super) async fn stream_by_doc_short_ids_impl(
        &self,
        collection_name: &str,
        doc_short_ids: &[u64],
        show_deleted: bool,
    ) -> query::error::Result<Box<dyn DocStream>> {
        let collection = self
            .db
            .get_collection(collection_name)
            .map_err(|e| query::error::QueryError::execution(format!("db error: {}", e)))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;

        let (_, has_migrations, preloaded_history) =
            self.load_migration_context(&collection).await?;

        let txn = self.db.new_txn(true).await.map_err(|e| {
            query::error::QueryError::execution(format!("failed to create txn: {}", e))
        })?;
        let datastore = txn.datastore().map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to get datastore for collection '{}': {}",
                collection_name, e
            ))
        })?;
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get systemstore: {}", e))
        })?;

        Ok(Box::new(LensedAutoCommitDocStream::new(
            Box::new(crate::collection::stream::ShortIdDocStream::new(
                collection.clone(),
                datastore,
                systemstore,
                doc_short_ids.to_vec(),
                show_deleted,
            )),
            Some(txn),
            self.clone_for_stream(),
            collection,
            has_migrations,
            preloaded_history,
        )))
    }
}

impl<S: Store + 'static> LensedAutoCommitFetcher<S> {
    pub(super) async fn stream_index_scan_impl(
        &self,
        collection_name: &str,
        params: &query::planner::index_selection::IndexScanParams,
        show_deleted: bool,
    ) -> query::error::Result<Option<Box<dyn DocStream>>> {
        use crate::read::index_stream::IndexDocStream;
        if !IndexDocStream::supports(params) {
            return Ok(None);
        }
        let collection = self
            .db
            .get_collection(collection_name)
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;
        let (_, has_migrations, preloaded_history) =
            self.load_migration_context(&collection).await?;
        let txn = self
            .db
            .new_txn(true)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
        let inner = IndexDocStream::open(
            collection.clone(),
            txn.datastore()
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?,
            txn.systemstore()
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?,
            params,
            show_deleted,
        )
        .await?;
        Ok(Some(Box::new(LensedAutoCommitDocStream::new(
            Box::new(inner),
            Some(txn),
            self.clone_for_stream(),
            collection,
            has_migrations,
            preloaded_history,
        ))))
    }
}
