//! DocFetcher trait method implementations (non-index-scan).

use bytes::Bytes;

use document::Document;
use query::fetcher::CommitsQueryOptions;
use query::runner::FetchByIdsResult;
use storage::corekv::Store;
use tracing::{debug, trace};

use crate::read::commits::{CommitsFetcher, CommitsQueryOptions as DbCommitsOptions};
use crate::read::versioned::VersionedFetcher;

use super::LensedAutoCommitFetcher;

impl<S: Store + 'static> LensedAutoCommitFetcher<S> {
    pub(super) async fn get_all_impl(
        &self,
        collection_name: &str,
    ) -> query::error::Result<Vec<Document>> {
        let collection = self
            .db
            .get_collection(collection_name)
            .map_err(|e| query::error::QueryError::execution(format!("db error: {}", e)))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;

        let (_, has_migrations, preloaded_history) =
            self.load_migration_context(&collection).await?;

        let target_version_id = &collection.schema().version_id;

        if has_migrations {
            debug!(
                collection = %collection_name,
                version_id = %target_version_id,
                "Collection has migrations registered"
            );
        }

        let txn = self.db.new_txn(true).await.map_err(|e| {
            query::error::QueryError::execution(format!("failed to create txn: {}", e))
        })?;

        let datastore = txn.datastore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get datastore: {}", e))
        })?;
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get systemstore: {}", e))
        })?;

        let read_result = collection
            .get_all_with_datastore(&datastore, &systemstore)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("storage error: {}", e)));

        drop(datastore);
        drop(systemstore);
        txn.discard().map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to discard lensed read transaction: {}",
                e
            ))
        })?;
        let docs = read_result?;
        trace!(
            collection = %collection_name,
            doc_count = docs.len(),
            "Fetched documents"
        );

        let mut processed_docs = Vec::with_capacity(docs.len());
        for doc in docs {
            let processed = self
                .process_document(doc, &collection, has_migrations, &preloaded_history)
                .await?
                .document;
            processed_docs.push(processed);
        }

        if !processed_docs.is_empty() {
            debug!(
                collection = %collection_name,
                returned = processed_docs.len(),
                "Lensed read completed"
            );
        }

        Ok(processed_docs)
    }

    pub(super) async fn get_all_with_deleted_impl(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> query::error::Result<Vec<(Document, bool)>> {
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
            query::error::QueryError::execution(format!("failed to get datastore: {}", e))
        })?;
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get systemstore: {}", e))
        })?;

        let read_result = collection
            .get_all_with_datastore_include_deleted(&datastore, &systemstore, show_deleted)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("storage error: {}", e)));

        drop(datastore);
        drop(systemstore);
        txn.discard().map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to discard lensed read transaction: {}",
                e
            ))
        })?;
        let docs_with_status = read_result?;
        let mut processed_docs = Vec::with_capacity(docs_with_status.len());
        for (doc, is_deleted) in docs_with_status {
            let processed = self
                .process_document(doc, &collection, has_migrations, &preloaded_history)
                .await?
                .document;
            processed_docs.push((processed, is_deleted));
        }

        Ok(processed_docs)
    }

    pub(super) async fn get_by_ids_impl(
        &self,
        collection_name: &str,
        doc_ids: &[String],
    ) -> query::error::Result<FetchByIdsResult> {
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
            query::error::QueryError::execution(format!("failed to get datastore: {}", e))
        })?;
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get systemstore: {}", e))
        })?;

        let read_result: query::error::Result<(Vec<Document>, Vec<String>)> = async {
            let mut docs = Vec::new();
            let mut missing_ids = Vec::new();

            for id_str in doc_ids {
                let doc_id = match document::DocID::from_string(id_str) {
                    Ok(id) => id,
                    Err(_) => {
                        missing_ids.push(id_str.clone());
                        continue;
                    }
                };

                match collection
                    .get_by_doc_id(&datastore, &systemstore, &doc_id)
                    .await
                    .map_err(|e| {
                        query::error::QueryError::execution(format!("storage error: {}", e))
                    })? {
                    Some(doc) => docs.push(doc),
                    None => missing_ids.push(id_str.clone()),
                }
            }

            Ok((docs, missing_ids))
        }
        .await;

        drop(datastore);
        drop(systemstore);
        txn.discard().map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to discard lensed read transaction: {}",
                e
            ))
        })?;
        let (docs, missing_ids) = read_result?;
        let mut processed_docs = Vec::with_capacity(docs.len());
        for doc in docs {
            let processed = self
                .process_document(doc, &collection, has_migrations, &preloaded_history)
                .await?
                .document;
            processed_docs.push(processed);
        }

        Ok(FetchByIdsResult::partial(processed_docs, missing_ids))
    }

    pub(super) async fn get_by_field_value_impl(
        &self,
        collection_name: &str,
        field_name: &str,
        value: &str,
    ) -> query::error::Result<Vec<Document>> {
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
            query::error::QueryError::execution(format!("failed to get datastore: {}", e))
        })?;
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get systemstore: {}", e))
        })?;

        let read_result = collection
            .get_all_with_datastore(&datastore, &systemstore)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("storage error: {}", e)));

        drop(datastore);
        drop(systemstore);
        txn.discard().map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to discard lensed read transaction: {}",
                e
            ))
        })?;
        let all_docs = read_result?;
        let mut processed_docs = Vec::new();
        for doc in all_docs {
            let processed = self
                .process_document(doc, &collection, has_migrations, &preloaded_history)
                .await?
                .document;
            let is_match = processed
                .get(field_name)
                .and_then(|v| v.as_str())
                .is_some_and(|field_value| field_value == value);
            if is_match {
                processed_docs.push(processed);
            }
        }

        Ok(processed_docs)
    }

    pub(super) async fn get_commits_impl(
        &self,
        options: &CommitsQueryOptions,
    ) -> query::error::Result<Vec<document::CommitRow>> {
        let db_options = DbCommitsOptions {
            doc_id: options.doc_id.clone(),
            cid: options.cid.clone(),
            depth: options.depth,
            height_start: options.height_start,
            height_end: options.height_end,
            field_name: options.field_name.clone(),
        };

        self.db
            .read_lent(
                |txn| async move { CommitsFetcher::new(txn).fetch_commits(&db_options).await },
            )
            .await
            .map_err(|e| {
                query::error::QueryError::execution(format!("failed to create txn: {}", e))
            })?
            .map_err(|e| query::error::QueryError::execution(format!("commits fetch error: {}", e)))
    }

    pub(super) async fn get_document_at_cid_impl(
        &self,
        cid: &str,
        expected_doc_id: Option<&str>,
        collection: &std::sync::Arc<schema::CollectionVersion>,
        caller_identity: Option<&identity::Did>,
    ) -> query::error::Result<Document> {
        let kms = self.db.kms();
        self.db
            .read_lent(|txn| async move {
                VersionedFetcher::with_kms(Some(txn), kms, caller_identity.cloned())
                    .get_document_at_cid(cid, expected_doc_id, collection.clone())
                    .await
            })
            .await
            .map_err(|e| {
                query::error::QueryError::execution(format!("failed to create txn: {}", e))
            })?
            .map_err(|e| query::error::QueryError::execution(e.to_string()))
    }

    pub(super) async fn get_documents_at_cid_impl(
        &self,
        collection: &std::sync::Arc<schema::CollectionVersion>,
        cid: &str,
        expected_doc_id: Option<&str>,
        caller_identity: Option<&identity::Did>,
    ) -> query::error::Result<Vec<Document>> {
        let kms = self.db.kms();
        self.db
            .read_lent(|txn| async move {
                VersionedFetcher::with_kms(Some(txn), kms, caller_identity.cloned())
                    .get_documents_at_cid(
                        cid,
                        expected_doc_id,
                        Some(collection.resolved_root_id()),
                        collection.clone(),
                    )
                    .await
            })
            .await
            .map_err(|e| {
                query::error::QueryError::execution(format!("failed to create txn: {}", e))
            })?
            .map_err(|e| query::error::QueryError::execution(e.to_string()))
    }

    pub(super) async fn get_view_cache_items_impl(
        &self,
        collection_id: u32,
    ) -> query::error::Result<Vec<Bytes>> {
        use storage::corekv::IterOptions;
        use storage::keys::datastore::ViewCacheKey;

        let txn = self.db.new_txn(true).await.map_err(|e| {
            query::error::QueryError::execution(format!("failed to create txn: {}", e))
        })?;

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

        let _ = txn.discard();

        Ok(items)
    }
}
