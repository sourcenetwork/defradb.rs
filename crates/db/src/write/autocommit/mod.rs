//! Auto-committing document mutator for non-transactional mutations.
//!
//! This mutator wraps a database and automatically creates and commits
//! a write transaction for each mutation operation. This enables mutations
//! without explicit transaction management while still providing proper
//! transactional semantics per operation.

pub mod batch;
mod create;
mod delete;
pub(crate) mod helpers;
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

use crate::block::builder::{write_collection_block, write_delete_block, write_document_blocks};
use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;
use crate::read::lensed::fetcher::LensedDocFetcher;
use crate::txn::DbTxn;

/// Captured per-mutation commit data: the document block (cid + bytes) plus,
/// for branchable collections, the collection block (cid + bytes).
pub(super) type CommitArtifacts = (Cid, Bytes, Option<(Cid, Bytes)>);
use defra_core::encryption::get_encryption_config;
use defra_core::signing::get_signing_config;

pub use batch::BatchMutator;

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

    /// Judge the composite a write built by the merge validator before its
    /// transaction commits, as `DbDocMutator` and `BatchMutator` do. The
    /// single-mutation path this mutator serves is the one a GraphQL request
    /// with one mutation, a REST document write and a backup import take,
    /// so without this most local writes in a governed collection committed
    /// unjudged. Nothing is read when no judge is installed.
    async fn judge_pending(
        &self,
        pending: Option<crate::merge::governance::PendingStores>,
        collection: &schema::CollectionVersion,
        doc_id: &str,
        cid: &Cid,
        block: &[u8],
    ) -> query::error::Result<()> {
        let Some(pending) = pending else {
            return Ok(());
        };
        crate::merge::governance::judge_local_write(
            &self.db, pending, collection, doc_id, cid, block,
        )
        .await
        .map_err(|error| query::error::QueryError::execution(error.to_string()))
    }

    /// The transaction's blockstore view a judgement reads the uncommitted
    /// blocks through, taken before any await so no borrow of the
    /// transaction is held across one; `None` when no judge is installed.
    fn pending_view(
        &self,
        txn: &DbTxn<S>,
    ) -> query::error::Result<Option<crate::merge::governance::PendingStores>> {
        if self.db.local_write_judge().is_none() {
            return Ok(None);
        }
        crate::merge::governance::PendingStores::of(txn)
            .map(Some)
            .map_err(|error| query::error::QueryError::execution(error.to_string()))
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
        self.create_impl(collection_name, doc).await
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
