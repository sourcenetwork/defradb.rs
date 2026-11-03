//! Migration context loading and document migration logic.

use rapidhash::{HashMapExt, RapidHashMap};
use std::sync::Arc;

use datastore::NamespaceView;
use document::Document;
use lens::{build_targeted_history, CollectionHistoryLink, LensDoc, TargetedHistoryLink};
use schema::CollectionVersion;
use storage::corekv::Store;
use tracing::debug;

use crate::collection::Collection;
use crate::definition::loader::get_collections_by_collection_id_known;
use crate::definition::migration::helpers::{
    cache_migrated_document_with_indexes, lens_doc_to_document,
};

use super::{History, LensedDocFetcher, VersionFacts};

impl<S: Store + 'static> LensedDocFetcher<S> {
    /// Check if any version in a list of collection versions has migrations registered.
    ///
    /// A collection has migrations if any version in its history has a transform
    /// configured in the previous_version field. This matches Go's behavior of
    /// checking the full history, not just the current version.
    fn versions_have_migrations(versions: &[CollectionVersion]) -> bool {
        for version in versions {
            if let Some(ref prev) = version.previous_version {
                if prev.transform.is_some() {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a collection has migrations registered (quick check).
    ///
    /// This is a fast check that only looks at the current version.
    /// For a full check, use `versions_have_migrations` with all loaded versions.
    #[allow(dead_code)]
    fn collection_has_migrations(collection: &Collection) -> bool {
        // Check if the current collection version has a previous version with a transform
        if let Some(ref prev) = collection.schema().previous_version {
            if prev.transform.is_some() {
                return true;
            }
        }
        false
    }

    /// Build the version history for a collection from a list of all versions.
    ///
    /// This takes all known versions and builds a directed graph showing
    /// the migration path to the target version.
    ///
    /// # Arguments
    /// * `versions` - All versions of the collection loaded from systemstore
    /// * `target_version_id` - The version to build the history toward
    fn build_collection_history_from_versions(
        versions: &[CollectionVersion],
        target_version_id: &str,
    ) -> Option<RapidHashMap<String, TargetedHistoryLink>> {
        if versions.is_empty() {
            return None;
        }

        let mut full_history: RapidHashMap<String, CollectionHistoryLink> = RapidHashMap::new();

        // First pass: build links with `previous` and `transform` from each
        // version's stored `previous_version` pointer.
        for version in versions {
            let mut link = CollectionHistoryLink::new(&version.version_id, &version.collection_id);

            // Check if there's a previous version
            if let Some(ref prev) = version.previous_version {
                link = link.with_previous(&prev.source_collection_id);
                if let Some(ref transform_id) = prev.transform {
                    link = link.with_transform(transform_id);
                }
            }

            full_history.insert(version.version_id.clone(), link);
        }

        // Second pass: backfill `next` so the targeted-history walk can
        // traverse forward from older versions. Without this, a node whose
        // collection sits at v1 cannot find a path to a doc that arrived at
        // v2 — the previous-only graph is unreachable from v1.
        // Mirrors the equivalent pass in `lensed::autocommit`.
        let reverse_links: Vec<(String, String)> = full_history
            .values()
            .flat_map(|link| {
                link.previous
                    .iter()
                    .map(|prev_id| (prev_id.clone(), link.version_id.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        for (parent_id, child_id) in reverse_links {
            if let Some(parent_link) = full_history.get_mut(&parent_id) {
                if !parent_link.next.contains(&child_id) {
                    parent_link.next.push(child_id);
                }
            }
        }

        build_targeted_history(&full_history, target_version_id)
    }

    /// Load all versions of a collection and check if any have migrations.
    ///
    /// Returns the list of versions and a boolean indicating if any have transforms.
    /// This matches Go's behavior of checking the full history for migrations.
    pub(super) async fn load_versions_and_check_migrations(
        &self,
        collection: &Collection,
    ) -> query::error::Result<(Arc<[CollectionVersion]>, bool)> {
        let collection_id = &collection.schema().collection_id;
        let generation = self.caches.version.generation();

        // Load all versions from systemstore
        let (readonly, systemstore) = {
            let txn = &*self.shared_txn()?;
            let readonly = txn
                .is_readonly()
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
            let systemstore = txn.systemstore().map_err(|e| {
                query::error::QueryError::execution(format!("failed to get systemstore: {}", e))
            })?;
            (readonly, systemstore)
        };
        if readonly {
            if let Some(facts) = self.caches.version.get(generation, collection_id) {
                return Ok(VersionFacts::clone(&facts));
            }
        }

        let versions =
            get_collections_by_collection_id_known(&systemstore, collection_id, Some(collection))
                .await
                .map_err(|e| {
                    query::error::QueryError::execution(format!(
                        "failed to load collection versions: {}",
                        e
                    ))
                })?;

        let has_migrations = Self::versions_have_migrations(&versions);
        let facts = (Arc::from(versions), has_migrations);
        if readonly {
            self.caches
                .version
                .publish(generation, collection_id, &Arc::new(facts.clone()));
        }
        Ok(facts)
    }

    /// Load full collection history from systemstore.
    ///
    /// This loads all versions of a collection and builds the targeted history graph.
    async fn load_collection_history(
        &self,
        collection: &Collection,
    ) -> query::error::Result<Arc<History>> {
        let collection_id = &collection.schema().collection_id;
        let target_version_id = &collection.schema().version_id;
        let cache_key = format!("{}:{}", collection_id, target_version_id);

        let generation = self.caches.history.generation();
        if let Some(history) = self.caches.history.get(generation, &cache_key) {
            return Ok(history);
        }

        // Load versions using the helper
        let (versions, _) = self.load_versions_and_check_migrations(collection).await?;

        if versions.is_empty() {
            return Err(query::error::QueryError::execution(format!(
                "no versions found for collection {}",
                collection_id
            )));
        }

        // Build the targeted history
        let history = Self::build_collection_history_from_versions(&versions, target_version_id)
            .ok_or_else(|| {
                query::error::QueryError::execution(format!(
                    "failed to build migration history for collection {}",
                    collection_id
                ))
            })?;

        let history = Arc::new(history);
        self.caches
            .history
            .publish(generation, &cache_key, &history);
        Ok(history)
    }

    /// Convert a Document to a LensDoc.
    pub fn doc_to_lens_doc(doc: &Document) -> Option<LensDoc> {
        // Use Document's to_map which handles all field conversions properly
        let map = doc.to_map().ok()?;

        // Convert RapidHashMap to serde_json::Map
        let mut lens_doc = LensDoc::new();
        for (key, value) in map {
            lens_doc.insert(key, value);
        }

        Some(lens_doc)
    }

    /// Check if a document needs migration to the target version.
    pub(super) fn doc_needs_migration(
        doc: &Document,
        target_version_id: &str,
        has_migrations: bool,
    ) -> bool {
        if !has_migrations {
            return false;
        }

        // Check if document's version differs from target
        doc.needs_migration(target_version_id)
    }

    /// Process a document, applying migration if needed.
    ///
    /// If the document's schema version matches the target, returns it unchanged.
    /// Otherwise, transforms it through the lens pipeline and caches the result.
    pub async fn process_document(
        &self,
        doc: Document,
        collection: &Collection,
        datastore: &NamespaceView,
        has_migrations: bool,
    ) -> query::error::Result<Document> {
        let target_version_id = &collection.schema().version_id;

        // Check if migration is needed
        if !Self::doc_needs_migration(&doc, target_version_id, has_migrations) {
            return Ok(doc);
        }

        let doc_version = doc.collection_version_id().unwrap_or("unknown").to_string();
        let doc_id_str = doc.id().map(|id| id.to_string()).unwrap_or_default();
        debug!(
            doc_id = ?doc.id(),
            from_version = %doc_version,
            to_version = %target_version_id,
            "Document needs migration"
        );

        // Load the collection history
        let history = self.load_collection_history(collection).await?;

        // Check if we have a migration path for this version
        if !history.contains_key(&doc_version) {
            debug!(
                doc_id = %doc_id_str,
                doc_version = %doc_version,
                target_version = %target_version_id,
                "Document version is unknown locally; returning it unchanged"
            );
            return Ok(doc);
        }

        // Convert document to LensDoc
        let original_lens_doc = Self::doc_to_lens_doc(&doc).ok_or_else(|| {
            query::error::QueryError::execution(format!(
                "failed to convert document {} to LensDoc for migration",
                doc_id_str
            ))
        })?;

        // Stay on the transaction's task: retry attempts are driven by the
        // engine I/O queue and cannot wait for a spawned Tokio lens task.
        let migrated_lens_doc = lens::transform_to_target(
            &self.lens_store,
            target_version_id,
            &history,
            lens::LensInput::new(&doc_version, original_lens_doc),
        )
        .await
        .map_err(|error| {
            query::error::QueryError::execution(format!(
                "lens migration failed for document {doc_id_str}: {error}"
            ))
        })?;

        debug!(
            doc_id = ?doc.id(),
            from_version = %doc_version,
            to_version = %target_version_id,
            "Document migration completed"
        );

        let migrated_doc = lens_doc_to_document(migrated_lens_doc, &doc, collection);
        self.update_datastore(datastore, collection, &doc, &migrated_doc)
            .await?;

        Ok(migrated_doc)
    }

    /// Update the datastore with migrated document values.
    ///
    /// Rust stores a document's fields in one CBOR blob, so this replaces that blob and updates
    /// the real schema-version key in the caller's transaction.
    ///
    /// Matches Go's `updateDataStore` in internal/lens/fetcher.go.
    async fn update_datastore(
        &self,
        datastore: &NamespaceView,
        collection: &Collection,
        _source_doc: &Document,
        migrated_doc: &Document,
    ) -> query::error::Result<()> {
        let txn = &*self.shared_txn()?;
        if txn.is_readonly().map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to inspect lens write-back transaction: {}",
                e
            ))
        })? {
            return Ok(());
        }
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to get systemstore for lens write-back: {}",
                e
            ))
        })?;

        cache_migrated_document_with_indexes(datastore, &systemstore, collection, migrated_doc)
            .await
            .map_err(|e| {
                query::error::QueryError::execution(format!(
                    "failed to cache migrated document: {}",
                    e
                ))
            })?;

        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../../tests/read/lensed_fetcher_caches.rs"]
mod cache_tests;

#[cfg(test)]
#[path = "../../../../tests/read/lensed_fetcher_snapshot_history.rs"]
mod snapshot_history_tests;
