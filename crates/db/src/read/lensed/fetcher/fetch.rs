//! DocFetcher trait method implementations.

use bytes::Bytes;
use document::Document;
use query::runner::FetchByIdsResult;
use storage::corekv::Store;
use tracing::{debug, trace};

use crate::collection::loader::get_collection_with_lazy_load;

use super::LensedDocFetcher;

async fn read_validated_exact(
    view: &datastore::NamespaceView,
    key: &[u8],
) -> query::error::Result<Option<Vec<u8>>> {
    let mut end = key.to_vec();
    end.push(0);
    let options = storage::corekv::IterOptions::new()
        .with_start(key.to_vec())
        .with_end(end)
        .with_validation(true);
    let mut iterator = view
        .iterator(options)
        .await
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    let mut value = None;
    while let Some(entry) = iterator
        .next()
        .await
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?
    {
        if entry.key.as_slice() == key {
            value = Some(entry.value.to_vec());
        }
    }
    Ok(value)
}

impl<S: Store + 'static> LensedDocFetcher<S> {
    pub(super) async fn get_by_unique_string_in_txn_impl(
        &self,
        collection_name: &str,
        field_name: &str,
        value: &str,
    ) -> query::error::Result<Option<(Document, std::sync::Arc<schema::CollectionVersion>)>> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;
        if field_name == "_docID" {
            use storage::corekv::Key;
            let mapping = storage::keys::doc_id_index::DocIDToDocRefKey::new(value).bytes();
            let Some(bytes) = read_validated_exact(&systemstore, &mapping).await? else {
                return Ok(None);
            };
            let reference = storage::keys::doc_id_index::DocRef::decode(&bytes)
                .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
            if reference.collection_short_id != collection.resolved_root_id() {
                return Ok(None);
            }
            let row = collection.doc_key(reference.doc_short_id);
            let deleted = collection.deleted_key(reference.doc_short_id);
            if read_validated_exact(&datastore, &row).await?.is_none()
                || read_validated_exact(&datastore, &deleted).await?.is_some()
            {
                return Ok(None);
            }
            let result = self
                .get_by_ids_impl(collection_name, &[value.to_owned()])
                .await?;
            let Some(doc) = result.into_docs().into_iter().next() else {
                return Ok(None);
            };
            if doc.id().is_none_or(|id| id.to_string() != value) {
                return Err(query::error::QueryError::execution(
                    "target document identity differs from selector",
                ));
            }
            return Ok(Some((
                doc,
                std::sync::Arc::new(collection.schema().clone()),
            )));
        }
        let field = collection
            .schema()
            .fields
            .iter()
            .find(|field| field.name == field_name)
            .ok_or_else(|| {
                query::error::QueryError::execution(
                    "create authorization target field does not exist",
                )
            })?;
        if !field.immutable
            || !matches!(
                &field.kind,
                schema::FieldKind::Scalar(
                    schema::ScalarKind::String | schema::ScalarKind::NillableString
                )
            )
        {
            return Err(query::error::QueryError::execution(
                "create authorization target field must be an immutable string",
            ));
        }
        let index = collection
            .schema()
            .indexes
            .iter()
            .find(|index| {
                index.unique && index.fields.len() == 1 && index.fields[0].name == field_name
            })
            .ok_or_else(|| {
                query::error::QueryError::execution(
                    "create authorization target field needs a unique index",
                )
            })?;
        let key = storage::keys::IndexDataStoreKey::new(
            collection.resolved_root_id(),
            index.id,
            vec![storage::keys::datastore::IndexedField::new(
                document::NormalValue::String(value.to_owned()),
                index.fields[0].descending,
            )],
        )
        .try_bytes()
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
        let Some(bytes) = read_validated_exact(&datastore, &key).await? else {
            return Ok(None);
        };
        let short_id = storage::keys::doc_id_index::decode_doc_short_id(&bytes)
            .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
        let doc_id = crate::docid::map::get_doc_id(&systemstore, short_id)
            .await
            .map_err(|error| query::error::QueryError::execution(error.to_string()))?
            .ok_or_else(|| {
                query::error::QueryError::execution("unique target has no document identity")
            })?;
        let result = self.get_by_ids_impl(collection_name, &[doc_id]).await?;
        let Some(doc) = result.into_docs().into_iter().next() else {
            return Ok(None);
        };
        if doc.get(field_name).and_then(|value| value.as_str()) != Some(value) {
            return Err(query::error::QueryError::execution(
                "unique target index does not match document",
            ));
        }
        Ok(Some((
            doc,
            std::sync::Arc::new(collection.schema().clone()),
        )))
    }

    /// Every live document, read for a decision on `deciding_on` when it is
    /// given (see `Collection::get_all_with_datastore_deciding_on`).
    pub(super) async fn get_all_impl(
        &self,
        collection_name: &str,
        deciding_on: Option<&rapidhash::RapidHashSet<String>>,
    ) -> query::error::Result<Vec<Document>> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;

        // Check if collection has migrations by loading full version history (matching Go)
        let (_, has_migrations) = self.load_versions_and_check_migrations(&collection).await?;
        let target_version_id = &collection.schema().version_id;

        if has_migrations {
            debug!(
                collection = %collection_name,
                version_id = %target_version_id,
                "Collection has migrations registered (full history check)"
            );
        }

        let docs = match deciding_on {
            Some(fields) => {
                collection
                    .get_all_with_datastore_deciding_on(&datastore, &systemstore, fields)
                    .await
            }
            None => {
                collection
                    .get_all_with_datastore(&datastore, &systemstore)
                    .await
            }
        }
        .map_err(|e| query::error::QueryError::execution(format!("storage error: {}", e)))?;

        // Count documents needing migration for logging
        let needs_migration_count = docs
            .iter()
            .filter(|doc| Self::doc_needs_migration(doc, target_version_id, has_migrations))
            .count();

        trace!(
            collection = %collection_name,
            doc_count = docs.len(),
            needs_migration = needs_migration_count,
            has_migrations = has_migrations,
            "Fetched documents"
        );

        // Process each document, applying migration if needed
        let mut processed_docs = Vec::with_capacity(docs.len());
        for doc in docs {
            let processed = self
                .process_document(doc, &collection, &datastore, has_migrations)
                .await?;
            processed_docs.push(processed);
        }

        if needs_migration_count > 0 {
            debug!(
                collection = %collection_name,
                migrated = needs_migration_count,
                total_docs = processed_docs.len(),
                "Documents migrated"
            );
        }

        Ok(processed_docs)
    }

    pub(super) async fn get_all_with_deleted_impl(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> query::error::Result<Vec<(Document, bool)>> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;

        // Check if collection has migrations by loading full version history (matching Go)
        let (_, has_migrations) = self.load_versions_and_check_migrations(&collection).await?;

        let docs_with_status = collection
            .get_all_with_datastore_include_deleted(&datastore, &systemstore, show_deleted)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("storage error: {}", e)))?;

        // Process each document, applying migration if needed
        let mut processed_docs = Vec::with_capacity(docs_with_status.len());
        for (doc, is_deleted) in docs_with_status {
            let processed = self
                .process_document(doc, &collection, &datastore, has_migrations)
                .await?;
            processed_docs.push((processed, is_deleted));
        }

        Ok(processed_docs)
    }

    pub(super) async fn get_by_ids_impl(
        &self,
        collection_name: &str,
        doc_ids: &[String],
    ) -> query::error::Result<FetchByIdsResult> {
        self.get_by_ids_impl_with_projection(collection_name, doc_ids, self.projection.as_ref())
            .await
    }

    pub(super) async fn get_by_ids_projected_impl(
        &self,
        collection_name: &str,
        doc_ids: &[String],
        fields: &rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<Option<FetchByIdsResult>> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;
        let storage_error = |error: crate::error::Error| {
            query::error::QueryError::execution(format!("storage error: {error}"))
        };

        // Field projections are exact only for documents written with the
        // current schema, which no lens migration touches. Historical or
        // legacy values require the full read and migration path. Downsample
        // validation reads the parts it decides on itself. Each document's
        // identity and version are read once, for the check and the read.
        let mut identities = Vec::with_capacity(doc_ids.len());
        for id in doc_ids {
            let identity = match document::DocID::from_string(id) {
                Ok(doc_id) => collection
                    .resolve_doc_identity(&systemstore, &doc_id)
                    .await
                    .map_err(storage_error)?,
                Err(_) => None,
            };
            let Some((short_id, canonical)) = identity else {
                identities.push((id, None));
                continue;
            };
            let version = collection
                .load_version(&datastore, short_id)
                .await
                .map_err(storage_error)?;
            if version.as_deref() != Some(collection.schema().version_id.as_str()) {
                return Ok(None);
            }
            identities.push((id, Some((short_id, canonical, version))));
        }

        let mut docs = Vec::with_capacity(identities.len());
        let mut missing_ids = Vec::new();
        for (id, identity) in identities {
            let fetched = match identity {
                Some((short_id, canonical, version)) => collection
                    .get_with_datastore_fields_at(&datastore, short_id, &canonical, fields, version)
                    .await
                    .map_err(storage_error)?,
                None => None,
            };
            match fetched {
                Some(doc) => docs.push(doc),
                None => missing_ids.push(id.clone()),
            }
        }

        self.finish_by_ids(
            collection_name,
            &collection,
            &datastore,
            doc_ids.len(),
            docs,
            missing_ids,
        )
        .await
        .map(Some)
    }

    async fn get_by_ids_impl_with_projection(
        &self,
        collection_name: &str,
        doc_ids: &[String],
        projection: Option<&rapidhash::RapidHashSet<String>>,
    ) -> query::error::Result<FetchByIdsResult> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;

        let mut docs = Vec::new();
        let mut missing_ids = Vec::new();

        for id_str in doc_ids {
            // Go DefraDB treats invalid doc IDs as "not found" rather than errors.
            // This matches behavior where querying for a non-existent ID returns empty results.
            let doc_id = match document::DocID::from_string(id_str) {
                Ok(id) => id,
                Err(_) => {
                    // Invalid doc ID format - treat as not found
                    missing_ids.push(id_str.clone());
                    continue;
                }
            };

            let fetched = if let Some(fields) = projection.or(self.projection.as_ref()) {
                match collection.resolve_doc_identity(&systemstore, &doc_id).await {
                    Ok(Some((short_id, canonical))) => {
                        collection
                            .get_with_datastore_fields(&datastore, short_id, &canonical, fields)
                            .await
                    }
                    Ok(None) => Ok(None),
                    Err(error) => Err(error),
                }
            } else {
                collection
                    .get_by_doc_id(&datastore, &systemstore, &doc_id)
                    .await
            };
            match fetched
                .map_err(|e| query::error::QueryError::execution(format!("storage error: {}", e)))?
            {
                Some(doc) => docs.push(doc),
                None => {
                    missing_ids.push(id_str.clone());
                }
            }
        }

        self.finish_by_ids(
            collection_name,
            &collection,
            &datastore,
            doc_ids.len(),
            docs,
            missing_ids,
        )
        .await
    }

    /// Migrate the documents a by-ID read found, and report the IDs it did not.
    async fn finish_by_ids(
        &self,
        collection_name: &str,
        collection: &crate::collection::Collection,
        datastore: &datastore::NamespaceView,
        requested: usize,
        docs: Vec<Document>,
        missing_ids: Vec<String>,
    ) -> query::error::Result<FetchByIdsResult> {
        // Check if collection has migrations by loading full version history (matching Go)
        let (_, has_migrations) = self.load_versions_and_check_migrations(collection).await?;
        let target_version_id = &collection.schema().version_id;

        // Count documents needing migration for logging
        let needs_migration_count = docs
            .iter()
            .filter(|doc| Self::doc_needs_migration(doc, target_version_id, has_migrations))
            .count();

        trace!(
            collection = %collection_name,
            requested,
            found = docs.len(),
            missing = missing_ids.len(),
            needs_migration = needs_migration_count,
            has_migrations = has_migrations,
            "Fetched documents by ID"
        );

        // Process each document, applying migration if needed
        let mut processed_docs = Vec::with_capacity(docs.len());
        for doc in docs {
            let processed = self
                .process_document(doc, collection, datastore, has_migrations)
                .await?;
            processed_docs.push(processed);
        }

        if needs_migration_count > 0 {
            debug!(
                collection = %collection_name,
                migrated = needs_migration_count,
                total_docs = processed_docs.len(),
                "Documents migrated"
            );
        }

        Ok(FetchByIdsResult::partial(processed_docs, missing_ids))
    }

    pub(super) async fn get_by_field_value_impl(
        &self,
        collection_name: &str,
        field_name: &str,
        value: &str,
    ) -> query::error::Result<Vec<Document>> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;

        // Check if collection has migrations by loading full version history (matching Go)
        let (_, has_migrations) = self.load_versions_and_check_migrations(&collection).await?;
        let target_version_id = &collection.schema().version_id;

        let all_docs = collection
            .get_all_with_datastore(&datastore, &systemstore)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("storage error: {}", e)))?;

        // Count documents needing migration for logging
        let needs_migration_count = all_docs
            .iter()
            .filter(|doc| Self::doc_needs_migration(doc, target_version_id, has_migrations))
            .count();

        trace!(
            collection = %collection_name,
            field = %field_name,
            value = %value,
            candidates = all_docs.len(),
            needs_migration = needs_migration_count,
            has_migrations = has_migrations,
            "Fetched documents by field value"
        );

        // Migrations may create or change the filtered field, so apply them
        // before evaluating the field value.
        let mut processed_docs = Vec::new();
        for doc in all_docs {
            let processed = self
                .process_document(doc, &collection, &datastore, has_migrations)
                .await?;
            let is_match = processed
                .get(field_name)
                .and_then(|v| v.as_str())
                .is_some_and(|field_value| field_value == value);
            if is_match {
                processed_docs.push(processed);
            }
        }

        if needs_migration_count > 0 {
            debug!(
                collection = %collection_name,
                migrated = needs_migration_count,
                total_docs = processed_docs.len(),
                "Documents migrated"
            );
        }

        Ok(processed_docs)
    }

    pub(super) async fn get_view_cache_items_impl(
        &self,
        collection_id: u32,
    ) -> query::error::Result<Vec<Bytes>> {
        use storage::corekv::IterOptions;
        use storage::keys::datastore::ViewCacheKey;

        let txn = &*self.shared_txn()?;

        let datastore = txn.datastore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get datastore: {}", e))
        })?;

        let prefix = ViewCacheKey::collection_prefix(collection_id);
        let opts = IterOptions::new().with_prefix(prefix);
        let mut iter = datastore.iterator(opts).await.map_err(|e| {
            query::error::QueryError::execution(format!("failed to iterate view cache: {}", e))
        })?;

        let mut items = Vec::new();
        while let Some(pair) = iter.next().await.map_err(|e| {
            query::error::QueryError::execution(format!("view cache iteration error: {}", e))
        })? {
            items.push(pair.value);
        }

        iter.close().await.map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to close view cache iterator: {}",
                e
            ))
        })?;

        Ok(items)
    }
}
