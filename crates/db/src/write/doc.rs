//! Document mutator for transaction-scoped mutations.

use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use document::{DocID, Document};
use query::mutator::{CreateResult, DeleteResult, DocMutator, UpdateResult};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use storage::corekv::{OnCommit, Store};

use crate::block::builder::DocRef;
use crate::block::builder::{write_delete_block_with_inputs, write_document_blocks_with_inputs};
use crate::collection::loader::{get_collection_with_index_manager, get_collection_with_lazy_load};
use crate::collection::Collection;
use crate::database::DB;
use crate::event::emission::register_update_event_callback;
use crate::txn::held::Held;
use crate::txn::{DbTxn, LentTxn};
use crate::write::autocommit::helpers::write_branchable_collection_block;
use defra_core::RequestContext;

fn document_json_value(doc: &Document) -> Option<serde_json::Value> {
    Some(serde_json::Value::Object(
        doc.to_map().ok()?.into_iter().collect(),
    ))
}

/// Document mutator that uses a database transaction.
///
/// This mutator holds a reference to an active transaction and uses the
/// transaction's collection cache with lazy loading.
///
/// # Ownership Model
///
/// The mutator holds a transaction lent to it ([`LentTxn`]): it shares the
/// transaction through `&self`, with no lock, with the other handles of one
/// request (a fetcher made with the same transaction, a collection
/// provider). [`take_txn`](Self::take_txn) gives it back once they are gone.
///
/// # Collection Access
///
/// Collections are loaded lazily from the SystemStore on first access within
/// the transaction. Once loaded, the collection metadata is cached for the
/// duration of the transaction. Note: This provides transaction-level caching,
/// not true snapshot isolation - if collections are accessed at different times,
/// they reflect the store state at the time of first access.
///
/// # Remote inputs
///
/// A write's remote inputs (embeddings, KMS keys, remote signatures) are never
/// fetched inside the transaction: they come from the mutator's memo of
/// prepared inputs (see [`DocMutator::with_prepared_inputs`]).
/// A miss ends the write with [`query::error::QueryError::NeedsInput`]; the
/// caller rolls the statement back to its savepoint, fetches the input with
/// [`DocMutator::resolve_prepared_input`], records it in the memo and runs the
/// statement again. A create keeps the short ID its first run drew, so the
/// inputs keyed by it are found on the next run.
pub struct DbDocMutator<S: Store + 'static> {
    db: Arc<DB<S>>,
    txn: Held<S>,
    broadcaster: Option<Arc<dyn crate::event::emission::TxnBroadcaster>>,
    prepared_inputs: query::prepared::PreparedInputMemo,
    /// Creates this attempt has made: the ordinal that names a create's
    /// short ID in the memo.
    creates: AtomicU64,
}

impl<S: Store + 'static> DbDocMutator<S> {
    /// Create a new transaction-scoped document mutator.
    ///
    /// Collections will be loaded lazily from the transaction's cache.
    pub fn new(db: Arc<DB<S>>, txn: DbTxn<S>) -> Self {
        Self::holding(db, Held::new(Arc::new(txn)), None)
    }

    /// Create a mutator that shares a transaction and (optionally) forwards
    /// committed writes to a `TxnBroadcaster`. When `broadcaster` is `Some`,
    /// each per-mutation `on_commit(OnCommit::Async(..))` callback both publishes to the
    /// local event bus and asks the broadcaster to push to P2P peers.
    pub fn from_shared_txn_with_broadcaster(
        db: Arc<DB<S>>,
        txn: LentTxn<S>,
        broadcaster: Option<Arc<dyn crate::event::emission::TxnBroadcaster>>,
    ) -> Self {
        Self::holding(db, Held::new(txn), broadcaster)
    }

    /// A mutator over a transaction lent to it, which its lender keeps and
    /// takes back: [`take_txn`](Self::take_txn) finds nothing.
    pub(crate) fn lent_with_broadcaster(
        db: Arc<DB<S>>,
        txn: &LentTxn<S>,
        broadcaster: Option<Arc<dyn crate::event::emission::TxnBroadcaster>>,
    ) -> Self {
        Self::holding(db, Held::borrowed(txn), broadcaster)
    }

    fn holding(
        db: Arc<DB<S>>,
        txn: Held<S>,
        broadcaster: Option<Arc<dyn crate::event::emission::TxnBroadcaster>>,
    ) -> Self {
        Self {
            db,
            txn,
            broadcaster,
            prepared_inputs: query::prepared::PreparedInputMemo::default(),
            creates: AtomicU64::new(0),
        }
    }

    /// Take the transaction out of the mutator (for commit/discard). `None`
    /// once it was taken, or while another handle still holds it.
    pub async fn take_txn(&self) -> Option<DbTxn<S>> {
        self.txn.take()
    }

    /// Check if the transaction has been consumed (via `take_txn()`).
    pub async fn is_consumed(&self) -> bool {
        self.txn.is_taken()
    }

    /// The transaction, for an operation or another handle to share.
    pub(crate) fn shared_txn(&self) -> query::error::Result<LentTxn<S>> {
        self.txn
            .get()
            .ok_or_else(|| query::error::QueryError::execution("transaction is no longer active"))
    }

    /// The storage identity of this attempt's next create. The short ID is
    /// drawn once per create of the statement and kept in the memo, so a run
    /// after a missing input gives each create the ID the first run gave it,
    /// and the KMS keys requested for that document are found.
    async fn next_create_identity(&self, collection: &Collection) -> query::error::Result<DocRef> {
        let ordinal = self.creates.fetch_add(1, Ordering::Relaxed);
        let request = query::prepared::PreparedInputRequest::new(
            query::prepared::PreparedInputKind::Allocation,
            [b"doc-short-id-v1".as_slice(), &ordinal.to_be_bytes()].concat(),
        );
        let drawn = match self.prepared_inputs.get(&request) {
            Some(drawn) => drawn,
            None => {
                let short_id = self
                    .db
                    .next_doc_short_id()
                    .await
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
                self.prepared_inputs
                    .insert(request, short_id.to_be_bytes().as_slice())
            }
        };
        let short_id = <[u8; 8]>::try_from(drawn.as_ref())
            .map(u64::from_be_bytes)
            .map_err(|_| {
                query::error::QueryError::execution("invalid prepared document short ID")
            })?;
        Ok(DocRef::new(collection.resolved_root_id(), short_id))
    }

    async fn ensure_collection_can_write(
        &self,
        collection_name: &str,
        collection: &Collection,
    ) -> query::error::Result<()> {
        let txn = &*self.shared_txn()?;
        txn.validate_collection_for_write(collection)
            .await
            .map_err(crate::error::commit_query_error)?;

        let was_created_in_txn = txn.was_collection_created(collection.collection_id());

        if was_created_in_txn {
            return Ok(());
        }

        let is_active = self
            .db
            .find_collection_by_id(collection.collection_id())
            .map_err(|e| query::error::QueryError::execution(format!("db error: {}", e)))?
            .is_some();

        if is_active {
            Ok(())
        } else {
            Err(query::error::QueryError::collection_not_found(
                collection_name,
            ))
        }
    }
}

impl<S: Store + 'static> DbDocMutator<S> {
    #[allow(clippy::too_many_arguments)]
    async fn register_update_callback(
        &self,
        collection_name: String,
        collection_id: String,
        doc_id: String,
        doc_cid: Cid,
        doc_block: Bytes,
        document_json: Option<serde_json::Value>,
        collection_block: Option<(Cid, Bytes)>,
        creator_did: Option<String>,
    ) -> query::error::Result<()> {
        let txn = &*self.shared_txn()?;
        register_update_event_callback(
            txn,
            self.db.event_bus(),
            self.broadcaster.as_ref(),
            self.db.local_commit_release(),
            collection_name,
            collection_id,
            doc_id,
            doc_cid,
            doc_block,
            document_json,
            collection_block,
            creator_did,
        )
        .map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to register tx update callback: {}",
                e
            ))
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> DocMutator for DbDocMutator<S> {
    fn supports_projected_updates(&self) -> bool {
        true
    }

    fn with_prepared_inputs(
        &self,
        inputs: query::prepared::PreparedInputMemo,
    ) -> Option<Arc<dyn DocMutator>> {
        Some(Arc::new(Self {
            db: Arc::clone(&self.db),
            txn: self.txn.get().map_or_else(Held::none, Held::new),
            broadcaster: self.broadcaster.clone(),
            prepared_inputs: inputs,
            creates: AtomicU64::new(0),
        }))
    }

    async fn resolve_prepared_input(
        &self,
        request: &query::prepared::PreparedInputRequest,
    ) -> query::error::Result<Arc<[u8]>> {
        self.resolve_prepared_input_as(&RequestContext::ambient(), request)
            .await
    }

    async fn resolve_prepared_input_as(
        &self,
        context: &RequestContext,
        request: &query::prepared::PreparedInputRequest,
    ) -> query::error::Result<Arc<[u8]>> {
        crate::write::prepare::resolve_input(&self.db, context, request).await
    }

    async fn resolve_prepared_inputs(
        &self,
        requests: Vec<query::prepared::PreparedInputRequest>,
    ) -> query::error::Result<Vec<Arc<[u8]>>> {
        self.resolve_prepared_inputs_as(&RequestContext::ambient(), requests)
            .await
    }

    async fn resolve_prepared_inputs_as(
        &self,
        context: &RequestContext,
        requests: Vec<query::prepared::PreparedInputRequest>,
    ) -> query::error::Result<Vec<Arc<[u8]>>> {
        crate::write::prepare::resolve_inputs(&self.db, context, requests).await
    }

    fn set_document_acp(&self, provider: Arc<dyn acp::DocumentACP>) {
        self.db.set_document_acp(provider);
    }

    async fn register_ordered_async_hook(
        &self,
        callback: storage::corekv::CommitHook,
    ) -> query::error::Result<()> {
        let txn = &*self.shared_txn()?;
        txn.on_commit(OnCommit::OrderedAsync(callback))
            .map_err(|error| query::error::QueryError::execution(error.to_string()))
    }

    async fn create(
        &self,
        collection_name: &str,
        doc: Document,
    ) -> query::error::Result<CreateResult> {
        self.create_as(&RequestContext::ambient(), collection_name, doc)
            .await
    }

    async fn create_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        mut doc: Document,
    ) -> query::error::Result<CreateResult> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::DocumentUpdate)
            .await
            .map_err(|e| query::error::QueryError::permission_denied(e.to_string()))?;

        let (collection, datastore, systemstore, index_manager) =
            get_collection_with_index_manager(&*self.shared_txn()?, collection_name).await?;
        self.db
            .check_node_access_in_txn(
                context,
                systemstore.sibling(storage::namespace::Namespace::Acpstore),
                acp::nac::NodePermission::DocumentUpdate,
            )
            .await
            .map_err(crate::error::commit_query_error)?;
        self.ensure_collection_can_write(collection_name, &collection)
            .await?;

        crate::search::embedding::set_embedding_from_inputs(
            &collection.schema().vector_embeddings,
            &mut doc,
            true,
            &rapidhash::RapidHashSet::default(),
            &self.db.options().embedding_config(),
            &self.prepared_inputs,
        )?;

        self.db
            .validate_downsample_write(&datastore, &systemstore, &collection, &doc, None)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

        let identity = self.next_create_identity(&collection).await?;
        let doc_short_id = identity.doc_short_id;

        // Blocks first: the public DocID is derived from the genesis composite
        // block CID (Go #4838).
        let (doc_id, doc_cid, doc_block, col_block_data) = {
            let txn = &*self.shared_txn()?;

            let blockstore = txn.blockstore().map_err(|e| {
                query::error::QueryError::execution(format!("failed to get blockstore: {}", e))
            })?;
            let headstore = txn.headstore().map_err(|e| {
                query::error::QueryError::execution(format!("failed to get headstore: {}", e))
            })?;

            let enc_config = context.encryption();
            let sign_config = context.signing();
            let kms = self.db.kms();

            let block_result = write_document_blocks_with_inputs(
                &blockstore,
                &headstore,
                &doc,
                collection.schema(),
                identity,
                None,
                enc_config,
                sign_config,
                context.batch_session(),
                kms.as_ref(),
                Some(&self.prepared_inputs),
                None,
            )
            .await
            .map_err(|error| {
                error.into_query_error_in(&format!(
                    "failed to write document blocks for transaction create on collection {collection_name}"
                ))
            })?;

            let doc_id = crate::write::autocommit::helpers::register_created_doc(
                self.db.document_acp(),
                context.creator(),
                &systemstore,
                &datastore,
                &collection,
                doc_short_id,
                &block_result,
            )
            .await?;
            doc.set_id(doc_id.clone());

            let col_block_data = write_branchable_collection_block(
                &self.db,
                collection_name,
                &collection,
                &blockstore,
                &headstore,
                block_result.cid,
                doc_short_id,
                context,
                Some(&self.prepared_inputs),
            )
            .await?;

            (doc_id, block_result.cid, block_result.block, col_block_data)
        };

        crate::write::autocommit::helpers::write_local_create(
            &datastore,
            &collection,
            &doc,
            doc_short_id,
            &index_manager,
        )
        .await?;

        self.register_update_callback(
            collection_name.to_string(),
            collection.collection_id().to_string(),
            doc_id.to_string(),
            doc_cid,
            doc_block,
            document_json_value(&doc),
            col_block_data,
            context.creator().map(str::to_owned),
        )
        .await?;

        Ok(CreateResult::new(doc_id, doc))
    }

    async fn update(
        &self,
        collection_name: &str,
        doc: Document,
        modified_fields: rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        self.update_as(
            &RequestContext::ambient(),
            collection_name,
            doc,
            modified_fields,
        )
        .await
    }

    async fn update_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        mut doc: Document,
        mut modified_fields: rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::DocumentUpdate)
            .await
            .map_err(|e| query::error::QueryError::permission_denied(e.to_string()))?;

        let (collection, datastore, systemstore, index_manager) =
            get_collection_with_index_manager(&*self.shared_txn()?, collection_name).await?;
        self.db
            .check_node_access_in_txn(
                context,
                systemstore.sibling(storage::namespace::Namespace::Acpstore),
                acp::nac::NodePermission::DocumentUpdate,
            )
            .await
            .map_err(crate::error::commit_query_error)?;
        self.ensure_collection_can_write(collection_name, &collection)
            .await?;

        let update_doc_id = doc
            .id()
            .cloned()
            .ok_or_else(|| query::error::QueryError::execution("update requires a document ID"))?;
        let (doc_short_id, canonical_doc_id) = collection
            .require_doc_identity(&systemstore, &update_doc_id)
            .await
            .map_err(|e| match e {
                crate::error::Error::DocumentNotFound(id) => {
                    query::error::QueryError::document_not_found(id)
                }
                other => query::error::QueryError::execution(other.to_string()),
            })?;
        doc.set_id(canonical_doc_id.clone());

        if context.encryption().is_none()
            && !self
                .db
                .is_downsample_source(collection_name)
                .map_err(|error| query::error::QueryError::execution(error.to_string()))?
            && collection
                .retain_changed_fields(
                    &datastore,
                    doc_short_id,
                    &doc,
                    &mut modified_fields,
                    query::update_parts::Run::First,
                )
                .await
                .map_err(crate::error::commit_query_error)?
        {
            return Ok(UpdateResult::new(doc, 0));
        }

        self.db
            .validate_downsample_write(
                &datastore,
                &systemstore,
                &collection,
                &doc,
                Some(&modified_fields),
            )
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

        let heads =
            crate::write::autocommit::helpers::load_update_heads(&datastore, doc_short_id).await?;
        crate::write::autocommit::helpers::write_local_update(
            &datastore,
            &collection,
            &mut doc,
            doc_short_id,
            &index_manager,
            &mut modified_fields,
            &heads,
            query::update_parts::Run::First,
            &self.db.options().embedding_config(),
            &self.prepared_inputs,
        )
        .await?;

        let (doc_cid, doc_block, col_block_data) = {
            let txn = &*self.shared_txn()?;

            let blockstore = txn.blockstore().map_err(|e| {
                query::error::QueryError::execution(format!("failed to get blockstore: {}", e))
            })?;
            let headstore = txn.headstore().map_err(|e| {
                query::error::QueryError::execution(format!("failed to get headstore: {}", e))
            })?;

            let enc_config = context.encryption();
            let sign_config = context.signing();
            let kms = self.db.kms();

            let block_result = write_document_blocks_with_inputs(
                &blockstore,
                &headstore,
                &doc,
                collection.schema(),
                DocRef::new(collection.resolved_root_id(), doc_short_id),
                Some(&modified_fields),
                enc_config,
                sign_config,
                context.batch_session(),
                kms.as_ref(),
                Some(&self.prepared_inputs),
                Some(heads),
            )
            .await
            .map_err(|error| {
                error.into_query_error_in(&format!(
                    "failed to write document blocks for transaction update on collection {collection_name}"
                ))
            })?;

            crate::write::autocommit::helpers::register_block_doc_id_mappings(
                &systemstore,
                &block_result,
                &canonical_doc_id.to_string(),
            )
            .await?;

            let col_block_data = write_branchable_collection_block(
                &self.db,
                collection_name,
                &collection,
                &blockstore,
                &headstore,
                block_result.cid,
                doc_short_id,
                context,
                Some(&self.prepared_inputs),
            )
            .await?;

            (block_result.cid, block_result.block, col_block_data)
        };

        if let Some(doc_id) = doc.id().cloned() {
            self.register_update_callback(
                collection_name.to_string(),
                collection.collection_id().to_string(),
                doc_id.to_string(),
                doc_cid,
                doc_block,
                document_json_value(&doc),
                col_block_data,
                context.creator().map(str::to_owned),
            )
            .await?;
        }

        let fields_modified = modified_fields.len();
        Ok(UpdateResult::new(doc, fields_modified))
    }

    async fn delete(
        &self,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<DeleteResult> {
        self.delete_as(&RequestContext::ambient(), collection_name, doc_id)
            .await
    }

    async fn delete_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<DeleteResult> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::DocumentDelete)
            .await
            .map_err(|e| query::error::QueryError::permission_denied(e.to_string()))?;

        let (collection, datastore, systemstore, index_manager) =
            get_collection_with_index_manager(&*self.shared_txn()?, collection_name).await?;
        self.db
            .check_node_access_in_txn(
                context,
                systemstore.sibling(storage::namespace::Namespace::Acpstore),
                acp::nac::NodePermission::DocumentDelete,
            )
            .await
            .map_err(crate::error::commit_query_error)?;
        self.ensure_collection_can_write(collection_name, &collection)
            .await?;

        let Some((doc_short_id, canonical_doc_id)) = collection
            .resolve_doc_identity(&systemstore, doc_id)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?
        else {
            return Ok(DeleteResult::new(doc_id.clone(), false));
        };

        let pre_delete_document_json = collection
            .get_with_datastore(&datastore, doc_short_id, &canonical_doc_id)
            .await
            .map_err(|e| {
                query::error::QueryError::execution(format!("pre-delete document read failed: {e}"))
            })?
            .and_then(|doc| document_json_value(&doc));

        let existed = collection
            .delete_with_indexes(&datastore, &canonical_doc_id, doc_short_id, &index_manager)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("delete error: {}", e)))?;

        if !existed {
            return Ok(DeleteResult::new(canonical_doc_id, existed));
        }
        crate::write::autocommit::helpers::unregister_deleted_doc(
            self.db.document_acp(),
            &systemstore,
            &collection,
            &canonical_doc_id,
        )
        .await?;

        let (doc_cid, doc_block, col_block_data) = {
            let txn = &*self.shared_txn()?;

            let blockstore = txn.blockstore().map_err(|e| {
                query::error::QueryError::execution(format!("failed to get blockstore: {}", e))
            })?;
            let headstore = txn.headstore().map_err(|e| {
                query::error::QueryError::execution(format!("failed to get headstore: {}", e))
            })?;

            let schema_version_id = collection.version_id();
            let sign_config = context.signing();

            let block_result = write_delete_block_with_inputs(
                &blockstore,
                &headstore,
                &canonical_doc_id.to_string(),
                doc_short_id,
                schema_version_id,
                sign_config,
                context.batch_session(),
                Some(&self.prepared_inputs),
                None,
            )
            .await
            .map_err(|error| {
                error.into_query_error_in(&format!(
                    "failed to write delete block for transaction delete on collection {collection_name}"
                ))
            })?;

            crate::docid::map::set_block_doc_id_mapping(
                &systemstore,
                &block_result.cid.to_string(),
                &canonical_doc_id.to_string(),
            )
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

            let col_block_data = write_branchable_collection_block(
                &self.db,
                collection_name,
                &collection,
                &blockstore,
                &headstore,
                block_result.cid,
                doc_short_id,
                context,
                Some(&self.prepared_inputs),
            )
            .await?;

            (block_result.cid, block_result.block, col_block_data)
        };

        self.register_update_callback(
            collection_name.to_string(),
            collection.collection_id().to_string(),
            canonical_doc_id.to_string(),
            doc_cid,
            doc_block,
            pre_delete_document_json,
            col_block_data,
            context.creator().map(str::to_owned),
        )
        .await?;

        Ok(DeleteResult::new(canonical_doc_id, existed))
    }

    async fn exists(&self, collection_name: &str, doc_id: &DocID) -> query::error::Result<bool> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;

        collection
            .exists_by_doc_id(&datastore, &systemstore, doc_id)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("exists error: {}", e)))
    }

    async fn get_for_update(
        &self,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<Option<Document>> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;

        collection
            .get_by_doc_id(&datastore, &systemstore, doc_id)
            .await
            .map_err(|e| {
                query::error::QueryError::execution(format!("get_for_update error: {}", e))
            })
    }
}
