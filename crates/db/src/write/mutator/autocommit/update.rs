use super::*;
use crate::write::create::TxnStores;
use crate::write::update::{embed_update, update_document, CounterWrite};

use query::runner::DocFetcher;

#[allow(clippy::type_complexity)]
impl<S: Store + 'static> AutoCommitMutator<S> {
    pub(super) async fn update_impl(
        &self,
        collection_name: &str,
        expected: Option<Document>,
        doc: Document,
        modified_fields: rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        self.db
            .check_node_access(None, acp::nac::NodePermission::DocumentUpdate)
            .await
            .map_err(|e| query::error::QueryError::permission_denied(e.to_string()))?;

        let (_collection_guard, collection) = self.guarded_collection(collection_name).await?;

        let mut doc = doc;
        let mut modified_fields = modified_fields;
        embed_update(&self.db, &collection, &mut doc, &mut modified_fields).await?;

        let input_doc_id = doc
            .id()
            .cloned()
            .ok_or_else(|| query::error::QueryError::execution("update requires a document ID"))?;
        let canonical_lock_id = {
            let identity_txn = self.db.new_txn(true).await.map_err(|e| {
                query::error::QueryError::execution(format!(
                    "failed to create identity transaction: {e}"
                ))
            })?;
            let canonical = {
                let systemstore = identity_txn.systemstore().map_err(|e| {
                    query::error::QueryError::execution(format!("failed to get systemstore: {e}"))
                })?;
                collection
                    .require_doc_identity(&systemstore, &input_doc_id)
                    .await
                    .map_err(|e| match e {
                        crate::error::Error::DocumentNotFound(id) => {
                            query::error::QueryError::document_not_found(id)
                        }
                        other => query::error::QueryError::execution(other.to_string()),
                    })?
                    .1
            };
            identity_txn.discard().map_err(|e| {
                query::error::QueryError::execution(format!(
                    "failed to discard identity transaction: {e}"
                ))
            })?;
            canonical
        };
        doc.set_id(canonical_lock_id.clone());

        // Serialize this write against concurrent merges (and other local writes)
        // touching the same document. Local counter increments and P2P merges both
        // read-modify-write the CRDT accumulation store; without this per-doc lock
        // their txns can race in a way the store's optimistic-conflict detection
        // does not always catch, dropping increments (#1021). The guard is held
        // across the whole write + commit.
        let _doc_guard = self
            .db
            .doc_write_queue()
            .acquire(&canonical_lock_id.to_string())
            .await;

        // The query plan materializes `doc` before this mutator acquires the
        // per-document write guard. A concurrent update can commit while this
        // call is waiting, leaving `doc` stale. Reload through the lensed
        // fetcher under the guard and rebase only the caller's declared patch;
        // otherwise an unrelated field from the concurrent update is silently
        // replaced by the stale snapshot.
        let fresh_fetcher =
            crate::LensedAutoCommitFetcher::new_without_write_back(Arc::clone(&self.db));
        let canonical_id = canonical_lock_id.to_string();
        let mut current_doc = fresh_fetcher
            .get_by_ids(collection_name, std::slice::from_ref(&canonical_id))
            .await?
            .into_docs()
            .into_iter()
            .next()
            .ok_or_else(|| query::error::QueryError::document_not_found(canonical_id))?;
        if let Some(expected) = expected {
            let values_unchanged = expected.values().len() == current_doc.values().len()
                && expected
                    .values()
                    .iter()
                    .all(|(field_name, expected_value)| {
                        current_doc.get(field_name) == Some(expected_value.value())
                    });
            if expected.id() != current_doc.id()
                || expected.is_deleted() != current_doc.is_deleted()
                || !values_unchanged
            {
                return Err(query::error::QueryError::transaction_conflict(
                    "transaction conflict. Please retry",
                ));
            }
        }
        for field_name in &modified_fields {
            if let Some(delta) = doc.get_counter_delta(field_name).cloned() {
                // Counter inputs are deltas computed from the stale
                // materialization. Carry the delta, not its provisional
                // accumulated value; `write_local_update` applies it to the
                // authoritative value below.
                current_doc.set_counter_delta(field_name.clone(), delta);
            } else if let Some(value) = doc.get(field_name).cloned() {
                current_doc.set(field_name.clone(), value);
            }
        }
        doc = current_doc;

        let txn = self.new_mutation_txn().await?;
        let stores = TxnStores::of(&txn);
        let result: query::error::Result<CommitArtifacts> = async {
            let stores = match &stores {
                Ok(stores) => stores,
                Err(e) => return Err(query::error::QueryError::execution(e.to_string())),
            };
            let index_manager = IndexManager::from_indexes(
                collection.resolved_root_id(),
                collection.schema(),
                collection.write_indexes(),
            )
            .map_err(|e| {
                query::error::QueryError::execution(format!(
                    "failed to create index manager for collection '{}': {}",
                    collection_name, e
                ))
            })?;

            let (doc_short_id, canonical_doc_id) = collection
                .require_doc_identity(&stores.systemstore, &input_doc_id)
                .await
                .map_err(|e| match e {
                    crate::error::Error::DocumentNotFound(id) => {
                        query::error::QueryError::document_not_found(id)
                    }
                    other => query::error::QueryError::execution(other.to_string()),
                })?;
            doc.set_id(canonical_doc_id);

            let updated = update_document(
                &self.db,
                stores,
                collection_name,
                &collection,
                &index_manager,
                &mut doc,
                doc_short_id,
                &modified_fields,
                CounterWrite::Now,
            )
            .await?;
            Ok((updated.cid, updated.block, updated.collection_block))
        }
        .await;
        drop(stores);

        let commit_result = self
            .finish_mutation(txn, result, collection_name, "update")
            .await?;

        if let Some(doc_id) = doc.id() {
            let (cid, block, col_data) = &commit_result;
            self.emit_update_events(
                &collection,
                &doc_id.to_string(),
                *cid,
                block.clone(),
                col_data.clone(),
            );
        }

        // Count modified fields
        let fields_modified = doc.values().len();
        let (cid, block, col_data) = commit_result;
        let mut result = UpdateResult::with_commit(doc, fields_modified, cid, block);
        if let Some((col_cid, col_bytes)) = col_data {
            result.broadcast_cid = Some(col_cid);
            result.broadcast_block = Some(col_bytes);
        }
        Ok(result)
    }
}
