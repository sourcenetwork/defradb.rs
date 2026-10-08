//! Auto-committing document mutator for non-transactional mutations.
//!
//! This mutator wraps a database and automatically creates and commits
//! a write transaction for each mutation operation. This enables mutations
//! without explicit transaction management while still providing proper
//! transactional semantics per operation.

mod create;
mod delete;
mod read;
pub mod update;

use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use document::{DocID, Document};
use events::{Message, Update};
use query::mutator::{CreateResult, DeleteResult, DocMutator, MutationBatch, UpdateResult};
use std::sync::{Arc, OnceLock};
use storage::corekv::Store;
use tracing::warn;

use crate::block::builder::write_delete_block;
use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;
use crate::read::lensed::fetcher::LensedDocFetcher;
use crate::txn::DbTxn;

/// Captured per-mutation commit data: the document block (cid + bytes) plus,
/// for branchable collections, the collection block (cid + bytes).
pub(super) type CommitArtifacts = (Cid, Bytes, Option<(Cid, Bytes)>);
use defra_core::signing::get_signing_config;

use crate::write::mutator::batch::BatchMutator;
use crate::write::persist::ensure_collection_is_active;

/// Document mutator that auto-commits transactions for each operation.
///
/// This is useful for mutations that don't need explicit transaction control.
/// Each operation creates a new write transaction, performs the mutation,
/// and commits (or discards on error).
///
/// # Transaction Semantics
///
/// Each mutation is atomic: it either succeeds entirely or fails without
/// partial changes. However, multiple mutations are NOT atomic with respect
/// to each other - if you need multiple operations to be atomic, use
/// `DbDocMutator` with explicit transaction management instead.
pub struct AutoCommitMutator<S: Store> {
    db: Arc<DB<S>>,
    document_acp: OnceLock<Arc<dyn acp::DocumentACP>>,
}

impl<S: Store> AutoCommitMutator<S> {
    /// Create a new auto-committing mutator wrapping the given database.
    pub fn new(db: Arc<DB<S>>) -> Self {
        Self {
            db,
            document_acp: OnceLock::new(),
        }
    }

    pub fn set_document_acp(&self, acp: Arc<dyn acp::DocumentACP>) {
        let _ = self.document_acp.set(acp);
    }

    async fn new_mutation_txn(&self) -> query::error::Result<DbTxn<S>> {
        self.db.new_txn(false).await.map_err(|error| {
            query::error::QueryError::execution(format!("failed to create txn: {error}"))
        })
    }

    async fn finish_mutation<T>(
        &self,
        txn: DbTxn<S>,
        result: query::error::Result<T>,
        collection_name: &str,
        operation: &str,
    ) -> query::error::Result<T> {
        match result {
            Ok(value) => {
                if let Err(error) = txn.commit().await {
                    warn!(
                        collection = %collection_name,
                        error = %error,
                        "Failed to commit transaction after {}",
                        operation
                    );
                    return Err(crate::error::commit_query_error(error));
                }
                Ok(value)
            }
            Err(error) => {
                if let Err(discard_error) = txn.discard() {
                    warn!(
                        collection = %collection_name,
                        error = %discard_error,
                        "Failed to discard transaction after {} error",
                        operation
                    );
                }
                Err(error)
            }
        }
    }

    async fn register_created_doc_with_acp(
        &self,
        collection: &Collection,
        doc_id: &str,
    ) -> query::error::Result<()> {
        let Some(policy) = collection.schema().policy.as_ref() else {
            return Ok(());
        };
        let Some(creator_did) = defra_core::signing::get_broadcast_creator_did() else {
            return Ok(());
        };
        let Some(acp) = self.document_acp.get() else {
            return Ok(());
        };
        let creator = identity::Did::new(&creator_did).map_err(|error| {
            query::error::QueryError::execution(format!("invalid mutation creator DID: {error}"))
        })?;

        if !acp
            .is_doc_registered(&policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(|error| {
                query::error::QueryError::execution(format!(
                    "failed to check ACP registration before publishing update: {error}"
                ))
            })?
        {
            acp.register_doc_object(&creator, &policy.id, &policy.resource_name, doc_id)
                .await
                .map_err(|error| {
                    query::error::QueryError::execution(format!(
                        "failed to register document with ACP before publishing update: {error}"
                    ))
                })?;
        }

        Ok(())
    }

    pub async fn new_batch_components(
        &self,
    ) -> query::error::Result<(Arc<BatchMutator<S>>, Arc<LensedDocFetcher<S>>)> {
        let txn = self.new_mutation_txn().await?;
        let fetcher = Arc::new(LensedDocFetcher::new(
            self.db.clone(),
            txn,
            self.db.lens_store.clone(),
            false,
        ));
        let batch = Arc::new(BatchMutator::new(self.db.clone(), fetcher.shared_txn()));
        Ok((batch, fetcher))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> DocMutator for AutoCommitMutator<S> {
    fn set_document_acp(&self, acp: Arc<dyn acp::DocumentACP>) {
        AutoCommitMutator::set_document_acp(self, acp);
    }

    async fn begin_batch(&self) -> query::error::Result<Option<MutationBatch>> {
        let (batch, fetcher) = self.new_batch_components().await?;
        let mutator: Arc<dyn DocMutator> = batch.clone();
        let controller: Arc<dyn query::mutator::MutationBatchController> = batch;
        Ok(Some(MutationBatch::new(mutator, fetcher, controller)))
    }

    async fn create(
        &self,
        collection_name: &str,
        doc: Document,
    ) -> query::error::Result<CreateResult> {
        let mut created = self.create_many_impl(collection_name, vec![doc]).await?;
        created
            .pop()
            .ok_or_else(|| query::error::QueryError::execution("create produced no document"))
    }

    async fn create_many(
        &self,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> query::error::Result<Vec<CreateResult>> {
        self.create_many_impl(collection_name, docs).await
    }

    async fn update(
        &self,
        collection_name: &str,
        doc: Document,
        modified_fields: rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        self.update_impl(collection_name, None, doc, modified_fields)
            .await
    }

    async fn update_if_unchanged(
        &self,
        collection_name: &str,
        expected: Document,
        doc: Document,
        modified_fields: rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        self.update_impl(collection_name, Some(expected), doc, modified_fields)
            .await
    }

    async fn delete(
        &self,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<DeleteResult> {
        self.delete_impl(collection_name, doc_id).await
    }

    async fn exists(&self, collection_name: &str, doc_id: &DocID) -> query::error::Result<bool> {
        self.exists_impl(collection_name, doc_id).await
    }

    async fn get_for_update(
        &self,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<Option<Document>> {
        self.get_for_update_impl(collection_name, doc_id).await
    }
}

impl<S: Store + 'static> AutoCommitMutator<S> {
    /// The collection's read guard, then its definition: resolved after the
    /// guard, so a patch or an index committed under the write guard is the
    /// definition this write uses.
    pub(super) async fn guarded_collection(
        &self,
        collection_name: &str,
    ) -> query::error::Result<(async_lock::RwLockReadGuardArc<()>, Collection)> {
        let guard = self
            .db
            .collection_read_guard_by_name(collection_name)
            .await
            .map_err(|error| query::error::QueryError::execution(error.to_string()))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;
        let collection = self.get_collection_or_err(collection_name)?;
        ensure_collection_is_active(&self.db, collection_name, &collection)?;
        Ok((guard, collection))
    }

    /// Get collection from DB cache or return a not-found error.
    pub(super) fn get_collection_or_err(
        &self,
        collection_name: &str,
    ) -> query::error::Result<Collection> {
        self.db
            .get_collection(collection_name)
            .map_err(|e| query::error::QueryError::execution(format!("db error: {}", e)))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))
    }

    /// Emit update events for subscriptions, carrying the actual block bytes
    /// so downstream consumers can traverse the DAG without an extra fetch.
    ///
    /// For branchable collections, emits a second event keyed by collection_id
    /// using the collection block's own cid/bytes (Go publishes the collection
    /// block separately at internal/db/collection.go:789).
    pub(super) fn emit_update_events(
        &self,
        collection: &Collection,
        doc_id_str: &str,
        doc_cid: Cid,
        doc_block: Bytes,
        collection_block: Option<(Cid, Bytes)>,
    ) {
        if let Some(bus) = self.db.event_bus() {
            let update = Update::new(
                doc_id_str.to_string(),
                doc_cid,
                collection.collection_id().to_string(),
                doc_block,
                false, // is_retry
                false, // is_relay (local mutation)
            );
            bus.publish(Message::update(update));

            if let Some((col_cid, col_block)) = collection_block {
                let col_update = Update::new_with_subject_doc_id(
                    String::new(), // empty doc_id → keyed by collection_id
                    doc_id_str.to_string(),
                    col_cid,
                    collection.collection_id().to_string(),
                    col_block,
                    false,
                    false,
                );
                bus.publish(Message::update(col_update));
            }
        }
    }
}
