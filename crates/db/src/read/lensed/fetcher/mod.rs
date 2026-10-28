//! Lensed document fetcher that applies schema migrations.
//!
//! This fetcher wraps an inner fetcher and applies lens transforms to documents
//! that are stored with older schema versions.
//!
//! # Migration Flow
//!
//! When a document is fetched:
//! 1. The fetcher loads the document with its stored schema version
//! 2. If the document's version differs from the target collection version
//!    and migrations are registered, the document is transformed
//! 3. Migrated values are cached in the datastore to avoid re-migration
//!
//! # Lazy Migration
//!
//! Documents are migrated on first read, not when schemas are updated.
//! This allows schema updates without rewriting all existing documents.
//! The migrated values and new version are cached in the datastore.

mod fetch;
pub mod migration;
mod scan;
pub mod stream;

use bytes::Bytes;
use rapidhash::RapidHashMap;
use std::sync::Arc;

use async_trait::async_trait;
use document::Document;
use lens::{TargetedHistoryLink, TransformStore};
use query::runner::{DocFetcher, FetchByIdsResult};
use storage::corekv::Store;

use crate::database::DB;
use crate::txn::held::Held;
use crate::txn::{DbTxn, LentTxn};

use super::generation_cache::GenerationCache;

/// A collection's versions and whether any of them has a migration.
type VersionFacts = (Arc<[schema::CollectionVersion]>, bool);

/// A collection version's targeted history, by version id.
type History = RapidHashMap<String, TargetedHistoryLink>;

/// What a transaction's fetchers derive from its schema: version facts by
/// collection id (cached for read-only transactions only, a writable one
/// always reloads its own schema writes) and targeted histories by
/// `"{collection_id}:{version_id}"`. Every request on the transaction shares
/// them, and each cache has generations of its own, which a schema change in
/// the transaction advances ([`invalidate`](Self::invalidate)).
#[derive(Clone, Default)]
pub(crate) struct FetcherCaches {
    version: Arc<GenerationCache<VersionFacts>>,
    history: Arc<GenerationCache<History>>,
}

impl FetcherCaches {
    /// Drops every value derived so far; one computed before this call is
    /// never cached after it.
    pub(crate) fn invalidate(&self) {
        self.history.invalidate();
        self.version.invalidate();
    }
}

/// Document fetcher that applies lens migrations to documents.
///
/// When documents are fetched from older schema versions, they are
/// transformed to the current (target) schema version using registered
/// lens migrations.
pub struct LensedDocFetcher<S: Store + 'static> {
    db: Arc<DB<S>>,
    txn: Held<S>,
    #[allow(dead_code)]
    lens_store: Arc<dyn TransformStore>,
    /// What the fetcher derived from its transaction's schema, shared with
    /// its stream clones and with the transaction's later requests.
    caches: FetcherCaches,
    /// Optional field projection used by mutation rebases whose dependencies
    /// are known. General query fetches leave this unset.
    projection: Option<rapidhash::RapidHashSet<String>>,
    prepared_inputs: Option<query::prepared::PreparedInputMemo>,
}

impl<S: Store + 'static> LensedDocFetcher<S> {
    /// Seed the per-version history cache directly.
    pub async fn insert_history(&self, key: String, history: History) {
        let generation = self.caches.history.generation();
        self.caches
            .history
            .publish(generation, &key, &Arc::new(history));
    }

    /// Create a new lensed document fetcher.
    ///
    /// # Arguments
    ///
    /// * `txn` - The database transaction
    /// * `lens_store` - The lens transform store for applying migrations
    #[allow(dead_code)]
    pub fn new(db: Arc<DB<S>>, txn: DbTxn<S>, lens_store: Arc<dyn TransformStore>) -> Self {
        Self::with_held(
            db,
            Held::new(Arc::new(txn)),
            lens_store,
            FetcherCaches::default(),
        )
    }

    /// A fetcher over a transaction lent to it, with what its earlier
    /// requests derived: whoever lent the transaction keeps it.
    pub(crate) fn lent(
        db: Arc<DB<S>>,
        txn: &LentTxn<S>,
        lens_store: Arc<dyn TransformStore>,
        caches: FetcherCaches,
    ) -> Self {
        Self::with_held(db, Held::borrowed(txn), lens_store, caches)
    }

    fn with_held(
        db: Arc<DB<S>>,
        txn: Held<S>,
        lens_store: Arc<dyn TransformStore>,
        caches: FetcherCaches,
    ) -> Self {
        Self {
            db,
            txn,
            lens_store,
            caches,
            projection: None,
            prepared_inputs: None,
        }
    }

    /// Resolve remote read keys through the request capsule outside retry attempts.
    pub(crate) fn with_prepared_inputs(
        mut self,
        inputs: Option<query::prepared::PreparedInputMemo>,
    ) -> Self {
        self.prepared_inputs = inputs;
        self
    }

    /// Create a fetcher for a mutation whose read dependencies are known.
    pub fn new_projected(
        db: Arc<DB<S>>,
        txn: DbTxn<S>,
        lens_store: Arc<dyn TransformStore>,
        fields: rapidhash::RapidHashSet<String>,
    ) -> Self {
        let mut fetcher = Self::new(db, txn, lens_store);
        fetcher.projection = Some(fields);
        fetcher
    }

    /// Build a lightweight fetcher sharing this one's transaction and caches,
    /// for use by a `DocStream` that outlives a single `&self` call.
    pub(super) fn stream_clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            txn: self.txn.get().map_or_else(Held::none, Held::new),
            lens_store: self.lens_store.clone(),
            caches: self.caches.clone(),
            projection: self.projection.clone(),
            prepared_inputs: self.prepared_inputs.clone(),
        }
    }

    /// Lets go of this handle's hold on the transaction: operations through it
    /// find none from then on.
    pub(super) fn release_txn(&mut self) {
        self.txn = Held::none();
    }

    /// Take the transaction out of the fetcher (for commit/discard). `None`
    /// once it was taken, or while another handle (a stream it opened) still
    /// holds it.
    pub async fn take_txn(&self) -> Option<DbTxn<S>> {
        self.txn.take()
    }

    /// Check if the transaction has been consumed.
    pub async fn is_consumed(&self) -> bool {
        self.txn.is_taken()
    }

    /// The transaction, for an operation or another handle to share.
    pub(crate) fn shared_txn(&self) -> query::error::Result<LentTxn<S>> {
        self.txn
            .get()
            .ok_or_else(|| query::error::QueryError::execution("transaction already consumed"))
    }

    /// Drops what the fetcher derived from its transaction's schema, after a
    /// schema change in the transaction; a value computed before this call is
    /// never cached after it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn invalidate_migration_cache(&self) {
        self.caches.invalidate();
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> DocFetcher for LensedDocFetcher<S> {
    async fn check_node_permission(
        &self,
        identity: &identity::Did,
        permission: acp::nac::NodePermission,
    ) -> query::error::Result<Option<bool>> {
        let Some(nac) = self.db.nac_manager() else {
            return Ok(None);
        };
        let txn = &*self.shared_txn()?;
        let view = txn
            .acpstore()
            .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
        nac.check_permission_in_txn(view, identity, permission)
            .await
            .map(Some)
            .map_err(|error| query::error::QueryError::execution(error.to_string()))
    }

    async fn transaction_acp(
        &self,
        provider: Arc<dyn acp::DocumentACP>,
    ) -> query::error::Result<Option<Arc<dyn acp::DocumentACP>>> {
        let txn = &*self.shared_txn()?;
        let view = txn
            .acpstore()
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
        Ok(provider.bind_transaction(view))
    }

    async fn scoped_acp(
        &self,
        provider: Arc<dyn acp::DocumentACP>,
    ) -> query::error::Result<Arc<dyn acp::DocumentACP>> {
        Ok(self
            .transaction_acp(provider.clone())
            .await?
            .unwrap_or(provider))
    }

    async fn get_all(&self, collection_name: &str) -> query::error::Result<Vec<Document>> {
        self.get_all_impl(collection_name, None).await
    }

    async fn get_all_projected(
        &self,
        collection_name: &str,
        fields: &rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<Vec<Document>> {
        self.get_all_impl(collection_name, Some(fields)).await
    }

    async fn get_all_with_deleted(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> query::error::Result<Vec<(Document, bool)>> {
        self.get_all_with_deleted_impl(collection_name, show_deleted)
            .await
    }

    async fn stream_all_with_deleted(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> query::error::Result<Box<dyn query::doc_stream::DocStream>> {
        self.stream_all_with_deleted_impl(collection_name, show_deleted)
            .await
    }

    async fn stream_index_scan(
        &self,
        collection_name: &str,
        params: &query::planner::index_selection::IndexScanParams,
        show_deleted: bool,
    ) -> query::error::Result<Option<Box<dyn query::doc_stream::DocStream>>> {
        self.stream_index_scan_impl(collection_name, params, show_deleted)
            .await
    }

    async fn stream_by_doc_short_ids(
        &self,
        collection_name: &str,
        doc_short_ids: &[u64],
        show_deleted: bool,
    ) -> query::error::Result<Box<dyn query::doc_stream::DocStream>> {
        self.stream_by_doc_short_ids_impl(collection_name, doc_short_ids, show_deleted)
            .await
    }

    async fn get_by_ids(
        &self,
        collection_name: &str,
        doc_ids: &[String],
    ) -> query::error::Result<FetchByIdsResult> {
        self.get_by_ids_impl(collection_name, doc_ids).await
    }

    async fn get_by_ids_projected(
        &self,
        collection_name: &str,
        doc_ids: &[String],
        fields: &rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<Option<FetchByIdsResult>> {
        self.get_by_ids_projected_impl(collection_name, doc_ids, fields)
            .await
    }

    async fn get_by_field_value(
        &self,
        collection_name: &str,
        field_name: &str,
        value: &str,
    ) -> query::error::Result<Vec<Document>> {
        self.get_by_field_value_impl(collection_name, field_name, value)
            .await
    }

    async fn get_document_arrivals(
        &self,
        options: &query::fetcher::DocumentArrivalOptions,
    ) -> query::error::Result<query::fetcher::DocumentArrivalPage> {
        let txn = &*self.shared_txn()?;
        crate::event::arrivals::read(txn, options)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))
    }

    async fn get_json_conflicts(
        &self,
        options: &query::fetcher::JsonConflictOptions,
    ) -> query::error::Result<Vec<query::fetcher::JsonConflict>> {
        let txn = &*self.shared_txn()?;
        crate::read::json_conflicts::read(txn, options)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))
    }

    async fn get_commits(
        &self,
        options: &query::fetcher::CommitsQueryOptions,
    ) -> query::error::Result<Vec<document::CommitRow>> {
        use crate::read::commits::{CommitsFetcher, CommitsQueryOptions as DbCommitsOptions};

        let db_options = DbCommitsOptions {
            doc_id: options.doc_id.clone(),
            cid: options.cid.clone(),
            depth: options.depth,
            height_start: options.height_start,
            height_end: options.height_end,
            field_name: options.field_name.clone(),
        };

        let commits_fetcher = CommitsFetcher::new(self.shared_txn()?);
        commits_fetcher
            .fetch_commits(&db_options)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("commits fetch error: {}", e)))
    }

    async fn search_fulltext_scored(
        &self,
        collection_name: &str,
        field_name: &str,
        query: &str,
    ) -> query::error::Result<rapidhash::RapidHashMap<String, f64>> {
        use crate::collection::loader::get_collection_with_lazy_load;
        use crate::index::IndexManager;

        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;

        let short_id = collection.resolved_root_id();
        let index_manager =
            IndexManager::from_indexes(short_id, collection.schema(), collection.write_indexes())
                .map_err(|e| {
                query::error::QueryError::execution(format!(
                    "failed to create index manager: {}",
                    e
                ))
            })?;

        let idx_name = crate::index::fulltext_index_name(field_name);
        let ft_index = index_manager
            .get_index(&idx_name)
            .and_then(|idx| idx.as_fulltext())
            .ok_or_else(|| {
                query::error::QueryError::execution(format!(
                    "fulltext index for field '{}' not found on collection '{}'",
                    field_name, collection_name
                ))
            })?;

        let scores = ft_index
            .search_scored(&datastore, query)
            .await
            .map_err(|e| {
                query::error::QueryError::execution(format!("fulltext search error: {}", e))
            })?;

        crate::docid::map::resolve_doc_id_scores(&systemstore, scores)
            .await
            .map_err(|e| {
                query::error::QueryError::execution(format!("doc ID resolution error: {}", e))
            })
    }

    async fn get_by_index_scan(
        &self,
        collection_name: &str,
        params: &query::planner::index_selection::IndexScanParams,
    ) -> query::error::Result<query::fetcher::IndexScanResult> {
        self.get_by_index_scan_impl(collection_name, params).await
    }

    async fn indexes_match_documents(&self, collection_name: &str) -> query::error::Result<bool> {
        let (collection, _, _) = crate::collection::loader::get_collection_with_lazy_load(
            &*self.shared_txn()?,
            collection_name,
        )
        .await?;
        Ok(!self
            .load_versions_and_check_migrations(&collection)
            .await?
            .1)
    }

    fn supports_index_queries(&self) -> bool {
        true
    }

    async fn estimate_index_scan(
        &self,
        collection_name: &str,
        params: &query::planner::index_selection::IndexScanParams,
        cap: u64,
    ) -> query::error::Result<Option<u64>> {
        self.estimate_index_scan_inner(collection_name, params, cap)
            .await
    }

    async fn vector_search(
        &self,
        collection_name: &str,
        index_id: u32,
        query_vector: &[f64],
        k: usize,
        effort: Option<usize>,
    ) -> query::error::Result<Vec<u64>> {
        let (collection, datastore, _) = crate::collection::loader::get_collection_with_lazy_load(
            &*self.shared_txn()?,
            collection_name,
        )
        .await?;
        let codes = self.shared_txn()?.vector_codes().clone();

        crate::read::vector::search_vector_index(
            &collection,
            &datastore,
            &codes,
            index_id,
            query_vector,
            k,
            effort,
        )
        .await
    }

    fn supports_vector_search(&self) -> bool {
        true
    }

    async fn get_document_at_cid(
        &self,
        cid: &str,
        expected_doc_id: Option<&str>,
        collection: &std::sync::Arc<schema::CollectionVersion>,
        caller_identity: Option<&identity::Did>,
    ) -> query::error::Result<Document> {
        use crate::read::versioned::VersionedFetcher;

        let versioned_fetcher = VersionedFetcher::with_kms(
            Some(self.shared_txn()?),
            self.db.kms(),
            caller_identity.cloned(),
        )
        .with_prepared_inputs(self.prepared_inputs.clone());
        versioned_fetcher
            .get_document_at_cid(cid, expected_doc_id, collection.clone())
            .await
            .map_err(|error| match error {
                crate::error::Error::Query(error) => error,
                error => query::error::QueryError::execution(error.to_string()),
            })
    }

    async fn get_documents_at_cid(
        &self,
        collection: &std::sync::Arc<schema::CollectionVersion>,
        cid: &str,
        expected_doc_id: Option<&str>,
        caller_identity: Option<&identity::Did>,
    ) -> query::error::Result<Vec<Document>> {
        use crate::read::versioned::VersionedFetcher;

        let versioned_fetcher = VersionedFetcher::with_kms(
            Some(self.shared_txn()?),
            self.db.kms(),
            caller_identity.cloned(),
        )
        .with_prepared_inputs(self.prepared_inputs.clone());
        versioned_fetcher
            .get_documents_at_cid(
                cid,
                expected_doc_id,
                Some(collection.resolved_root_id()),
                collection.clone(),
            )
            .await
            .map_err(|error| match error {
                crate::error::Error::Query(error) => error,
                error => query::error::QueryError::execution(error.to_string()),
            })
    }

    async fn get_view_cache_items(&self, collection_id: u32) -> query::error::Result<Vec<Bytes>> {
        self.get_view_cache_items_impl(collection_id).await
    }
}
