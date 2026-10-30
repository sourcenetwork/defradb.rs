//! Document mutator abstraction for mutation execution.
//!
//! This module provides the `DocMutator` trait which abstracts storage write
//! operations for mutation execution, following the same pattern as `DocFetcher`.

use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use defra_core::RequestContext;
use document::{DocID, Document};
use identity::Did;
use rapidhash::RapidHashSet;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::{future::Future, pin::Pin};
use storage::corekv::MaybeSendSync;

use crate::error::Result;
use crate::Filter;

/// Collection-level truncate operation used by GraphQL mutations.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait CollectionTruncator: MaybeSendSync {
    async fn truncate(
        &self,
        collection_name: &str,
        filter: Option<Filter>,
        identity: Option<&Did>,
    ) -> Result<()>;

    /// [`Self::truncate`] for the request `context` serves. The default runs
    /// `truncate` for the request's creator inside [`RequestContext::mirror`].
    async fn truncate_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        filter: Option<Filter>,
    ) -> Result<()> {
        let creator = context.creator().and_then(|did| Did::new(did).ok());
        context
            .mirror(self.truncate(collection_name, filter, creator.as_ref()))
            .await
    }
}

/// Status of P2P broadcast after a mutation.
///
/// This allows callers to know whether changes were successfully broadcast
/// to the P2P network, enabling appropriate handling of partial success.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum BroadcastStatus {
    /// Broadcast succeeded
    Success,
    /// Broadcast failed with the given error message
    Failed(String),
    /// Broadcast spawned but not yet complete (fire-and-forget)
    Pending,
    /// Broadcast was not attempted (P2P not enabled or not applicable)
    #[default]
    NotAttempted,
}

impl BroadcastStatus {
    /// Returns true if the broadcast succeeded.
    pub fn is_success(&self) -> bool {
        matches!(self, BroadcastStatus::Success)
    }

    /// Returns true if the broadcast failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, BroadcastStatus::Failed(_))
    }

    /// Returns the error message if the broadcast failed.
    pub fn error(&self) -> Option<&str> {
        match self {
            BroadcastStatus::Failed(msg) => Some(msg),
            _ => None,
        }
    }
}

/// A document a write changed besides its own, such as a `@chunk` chunk.
#[derive(Debug, Clone)]
pub struct DerivedWrite {
    pub collection_name: String,
    /// The collection's ID, which its blocks are published under.
    pub collection_id: String,
    pub doc_id: DocID,
    /// The created document, or `None` for a delete.
    pub document: Option<Document>,
    /// The CID of the committed composite block.
    pub commit_cid: Cid,
    /// The committed composite block's bytes.
    pub commit_block: Bytes,
}

/// Result of a create mutation, including the generated DocID and the document.
#[derive(Debug, Clone)]
pub struct CreateResult {
    /// The generated document ID
    pub doc_id: DocID,
    /// The created document (with ID set)
    pub document: Document,
    /// Status of P2P broadcast (if applicable)
    pub broadcast_status: BroadcastStatus,
    /// The CID of the commit block (for _version queries)
    /// This is the dag-cbor encoded Block CID, not the document data CID.
    pub commit_cid: Option<Cid>,
    /// The raw bytes of the committed composite block (for P2P broadcast)
    pub commit_block: Option<Bytes>,
    /// For branchable collections: the collection block CID to broadcast instead of composite.
    pub broadcast_cid: Option<Cid>,
    /// For branchable collections: the collection block bytes to broadcast.
    pub broadcast_block: Option<Bytes>,
    /// Other documents this write created or deleted, for the broadcast wrapper to publish.
    pub derived: Vec<DerivedWrite>,
}

impl CreateResult {
    /// Create a new result.
    pub fn new(doc_id: DocID, document: Document) -> Self {
        Self {
            doc_id,
            document,
            broadcast_status: BroadcastStatus::NotAttempted,
            commit_cid: None,
            commit_block: None,
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }

    /// Create a result with commit CID and block data (for _version + P2P broadcast).
    pub fn with_commit(
        doc_id: DocID,
        document: Document,
        commit_cid: Cid,
        commit_block: Bytes,
    ) -> Self {
        Self {
            doc_id,
            document,
            broadcast_status: BroadcastStatus::NotAttempted,
            commit_cid: Some(commit_cid),
            commit_block: Some(commit_block),
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }

    /// Create a result with broadcast status.
    pub fn with_broadcast(
        doc_id: DocID,
        document: Document,
        broadcast_status: BroadcastStatus,
    ) -> Self {
        Self {
            doc_id,
            document,
            broadcast_status,
            commit_cid: None,
            commit_block: None,
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }

    /// Create a result with commit CID and broadcast status.
    pub fn with_commit_and_broadcast(
        doc_id: DocID,
        document: Document,
        commit_cid: Cid,
        commit_block: Bytes,
        broadcast_status: BroadcastStatus,
    ) -> Self {
        Self {
            doc_id,
            document,
            broadcast_status,
            commit_cid: Some(commit_cid),
            commit_block: Some(commit_block),
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }
}

/// Result of an update mutation.
#[derive(Debug, Clone)]
pub struct UpdateResult {
    /// The updated document
    pub document: Document,
    /// Number of fields that were modified
    pub fields_modified: usize,
    /// Status of P2P broadcast (if applicable)
    pub broadcast_status: BroadcastStatus,
    /// The CID of the committed composite block (for P2P broadcast)
    pub commit_cid: Option<Cid>,
    /// The raw bytes of the committed composite block (for P2P broadcast)
    pub commit_block: Option<Bytes>,
    /// For branchable collections: the collection block CID to broadcast instead of composite.
    pub broadcast_cid: Option<Cid>,
    /// For branchable collections: the collection block bytes to broadcast.
    pub broadcast_block: Option<Bytes>,
    /// Other documents this write created or deleted, for the broadcast wrapper to publish.
    pub derived: Vec<DerivedWrite>,
}

impl UpdateResult {
    /// Create a new result.
    pub fn new(document: Document, fields_modified: usize) -> Self {
        Self {
            document,
            fields_modified,
            broadcast_status: BroadcastStatus::NotAttempted,
            commit_cid: None,
            commit_block: None,
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }

    /// Create a result with committed block data (for P2P broadcast).
    pub fn with_commit(
        document: Document,
        fields_modified: usize,
        commit_cid: Cid,
        commit_block: Bytes,
    ) -> Self {
        Self {
            document,
            fields_modified,
            broadcast_status: BroadcastStatus::NotAttempted,
            commit_cid: Some(commit_cid),
            commit_block: Some(commit_block),
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }

    /// Create a result with broadcast status.
    pub fn with_broadcast(
        document: Document,
        fields_modified: usize,
        broadcast_status: BroadcastStatus,
    ) -> Self {
        Self {
            document,
            fields_modified,
            broadcast_status,
            commit_cid: None,
            commit_block: None,
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }
}

/// Result of a delete mutation.
#[derive(Debug, Clone)]
pub struct DeleteResult {
    /// The document ID that was deleted
    pub doc_id: DocID,
    /// Whether the document existed before deletion
    pub existed: bool,
    /// Status of P2P broadcast (if applicable)
    pub broadcast_status: BroadcastStatus,
    /// The CID of the committed composite delete block (for P2P broadcast)
    pub commit_cid: Option<Cid>,
    /// The raw bytes of the committed composite delete block (for P2P broadcast)
    pub commit_block: Option<Bytes>,
    /// For branchable collections: the CID of the collection-level head block
    /// that was written alongside the composite delete (for P2P broadcast).
    /// Go emits a second update for this so collection subscribers / replicators
    /// don't miss the collection head on deletes.
    pub broadcast_cid: Option<Cid>,
    /// For branchable collections: the raw bytes of the collection-level head
    /// block written alongside the composite delete (for P2P broadcast).
    pub broadcast_block: Option<Bytes>,
    /// Other documents this write created or deleted, for the broadcast wrapper to publish.
    pub derived: Vec<DerivedWrite>,
}

impl DeleteResult {
    /// Create a new result.
    pub fn new(doc_id: DocID, existed: bool) -> Self {
        Self {
            doc_id,
            existed,
            broadcast_status: BroadcastStatus::NotAttempted,
            commit_cid: None,
            commit_block: None,
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }

    /// Create a result with committed block data (for P2P broadcast).
    pub fn with_commit(doc_id: DocID, existed: bool, commit_cid: Cid, commit_block: Bytes) -> Self {
        Self {
            doc_id,
            existed,
            broadcast_status: BroadcastStatus::NotAttempted,
            commit_cid: Some(commit_cid),
            commit_block: Some(commit_block),
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }

    /// Create a result with broadcast status.
    pub fn with_broadcast(doc_id: DocID, existed: bool, broadcast_status: BroadcastStatus) -> Self {
        Self {
            doc_id,
            existed,
            broadcast_status,
            commit_cid: None,
            commit_block: None,
            broadcast_cid: None,
            broadcast_block: None,
            derived: Vec::new(),
        }
    }
}

/// Controller for a request-scoped mutation batch.
///
/// Implementations own the shared transaction lifecycle for an implicit
/// GraphQL mutation request and are responsible for committing or rolling
/// back all writes performed through the paired mutator/fetcher.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait MutationBatchController: MaybeSendSync {
    /// Execute the complete implicit request operation against one batch attempt.
    /// Implementations with engine-owned retries override this method and call
    /// `operation` once per fresh attempt. The default preserves the ordinary
    /// single-transaction behavior.
    async fn run(
        &self,
        operation: MutationBatchOperation,
        mutator: Option<Arc<dyn DocMutator>>,
        fetcher: Option<Arc<dyn crate::fetcher::DocFetcher>>,
    ) -> Result<JsonValue> {
        let mutator = mutator
            .ok_or_else(|| crate::error::QueryError::execution("mutation batch has no mutator"))?;
        let fetcher = fetcher
            .ok_or_else(|| crate::error::QueryError::execution("mutation batch has no fetcher"))?;
        match operation(mutator, fetcher).await {
            Ok(value) => {
                self.commit().await?;
                Ok(value)
            }
            Err(error) => {
                if let Err(discard_error) = self.discard().await {
                    tracing::warn!(error = %discard_error, "Failed to discard implicit mutation batch");
                }
                Err(error)
            }
        }
    }

    /// Commit the shared transaction backing this batch.
    async fn commit(&self) -> Result<()>;

    /// Roll back the shared transaction backing this batch.
    async fn discard(&self) -> Result<()>;
}

/// One execution of the parsed mutation sequence with an attempt-local mutator
/// and fetcher. The operation must rebuild all per-attempt result state each time
/// it is called; request preparation remains captured and reusable.
#[cfg(not(target_arch = "wasm32"))]
pub type MutationBatchOperation = Arc<
    dyn Fn(Arc<dyn DocMutator>, Arc<dyn crate::fetcher::DocFetcher>) -> MutationBatchFuture
        + Send
        + Sync,
>;
#[cfg(target_arch = "wasm32")]
pub type MutationBatchOperation =
    Arc<dyn Fn(Arc<dyn DocMutator>, Arc<dyn crate::fetcher::DocFetcher>) -> MutationBatchFuture>;

#[cfg(not(target_arch = "wasm32"))]
pub type MutationBatchFuture = Pin<Box<dyn Future<Output = Result<JsonValue>> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
pub type MutationBatchFuture = Pin<Box<dyn Future<Output = Result<JsonValue>> + 'static>>;

/// A request-scoped mutation batch with a shared mutator and fetcher.
pub struct MutationBatch {
    mutator: Option<Arc<dyn DocMutator>>,
    fetcher: Option<Arc<dyn crate::fetcher::DocFetcher>>,
    controller: Arc<dyn MutationBatchController>,
}

impl MutationBatch {
    /// Create a new mutation batch wrapper.
    pub fn new(
        mutator: Arc<dyn DocMutator>,
        fetcher: Arc<dyn crate::fetcher::DocFetcher>,
        controller: Arc<dyn MutationBatchController>,
    ) -> Self {
        Self {
            mutator: Some(mutator),
            fetcher: Some(fetcher),
            controller,
        }
    }

    /// Create a batch whose controller creates fresh mutator/fetcher views for
    /// each attempt.
    pub fn new_retryable(controller: Arc<dyn MutationBatchController>) -> Self {
        Self {
            mutator: None,
            fetcher: None,
            controller,
        }
    }

    /// Get the shared mutator for this batch.
    pub fn mutator(&self) -> Arc<dyn DocMutator> {
        self.mutator
            .clone()
            .expect("retryable batches run an operation")
    }

    /// Get the shared fetcher for this batch.
    pub fn fetcher(&self) -> Arc<dyn crate::fetcher::DocFetcher> {
        self.fetcher
            .clone()
            .expect("retryable batches run an operation")
    }

    /// Run a complete top-level mutation request through the batch controller.
    pub async fn run(&self, operation: MutationBatchOperation) -> Result<JsonValue> {
        self.controller
            .run(operation, self.mutator.clone(), self.fetcher.clone())
            .await
    }

    /// Commit the batch transaction.
    pub async fn commit(&self) -> Result<()> {
        self.controller.commit().await
    }

    /// Roll back the batch transaction.
    pub async fn discard(&self) -> Result<()> {
        self.controller.discard().await
    }
}

/// Storage abstraction for mutating documents.
///
/// This trait provides write operations for mutations, complementing `DocFetcher`
/// which provides read operations. Implementations are expected to be
/// transaction-scoped, meaning all operations occur within a single transaction.
///
/// Each write comes twice: `*_as` takes the [`RequestContext`] of the request
/// it serves, and how its blocks are encrypted, signed and attributed comes
/// from there; the plain form runs in [`RequestContext::ambient`]. A storage
/// mutator overrides both forms of every write it implements. The defaults of
/// `create_as`, `update_as`, `delete_as` and the preparation calls run the
/// plain form inside [`RequestContext::mirror`], so a mutator that implements
/// only the plain form still serves a request; the defaults of the composite
/// writes (`create_many_as`, `update_if_unchanged_as`,
/// `update_if_fields_unchanged_as`) compose the `*_as` writes, as the plain
/// defaults compose the plain ones.
///
/// # Transaction Semantics
///
/// All mutations performed through a `DocMutator` should be atomic within
/// the transaction context. The caller is responsible for committing or
/// rolling back the transaction after mutations complete.
///
/// # Example
///
/// ```ignore
/// async fn create_user(mutator: &dyn DocMutator, name: &str, age: i64) -> Result<DocID> {
///     let mut doc = Document::new();
///     doc.set("name", name);
///     doc.set("age", age);
///
///     let result = mutator.create_as(&RequestContext::default(), "Users", doc).await?;
///     Ok(result.doc_id)
/// }
/// ```
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait DocMutator: MaybeSendSync {
    /// The CRDT types of the node whose documents this mutator writes: each
    /// field's write goes through its type.
    fn crdts(&self) -> crdt::Crdts;

    /// Whether conflicts from mutation batches returned by this mutator are
    /// retried by the storage attempt driver.
    fn engine_retries_batches(&self) -> bool {
        false
    }

    /// Return the active attempt's external ACP registration projection, if any.
    fn acp_registration_projection(&self) -> Option<Arc<crate::txn::AcpRegistrationProjection>> {
        None
    }

    /// Return a request-bound mutator that can see prepared external inputs
    /// while executing this request. Implementations must return an isolated
    /// clone/context; this must not mutate shared mutator state.
    fn with_prepared_inputs(
        &self,
        _inputs: crate::prepared::PreparedInputMemo,
    ) -> Option<Arc<dyn DocMutator>> {
        None
    }

    /// Whether this mutator resolves persistent inputs before request retries.
    fn requires_write_preparation(&self) -> bool {
        false
    }

    /// [`Self::requires_write_preparation`] for the request `context` serves.
    fn requires_write_preparation_as(&self, _context: &RequestContext) -> bool {
        self.requires_write_preparation()
    }

    /// Whether update reads may be projected to the mutation's known field
    /// dependencies. Documents remain fully materialized, but fields outside
    /// that dependency set are not validated. Implementations opting in must
    /// preserve result/event semantics and register every additional dependency
    /// their preparation and update logic inspects. Custom mutators retain full
    /// reads unless they explicitly opt in.
    fn supports_projected_updates(&self) -> bool {
        false
    }

    /// Which run of its request this mutator's updates make
    /// ([`crate::update_parts::Run`]): a mutator of an attempt that runs
    /// again after losing a race on a part says so, and the parts its updates
    /// decide on follow.
    fn update_run(&self) -> crate::update_parts::Run {
        crate::update_parts::Run::First
    }

    /// Resolve persistent inputs without staging a document mutation.
    async fn prepare_write(
        &self,
        _collection_name: &str,
        doc: Document,
        _modified_fields: Option<rapidhash::RapidHashSet<String>>,
        _encryption: Option<defra_core::encryption::EncryptionConfig>,
    ) -> Result<Document> {
        Ok(doc)
    }

    /// [`Self::prepare_write`] for the request `context` serves, encrypting
    /// as its context says.
    async fn prepare_write_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc: Document,
        modified_fields: Option<rapidhash::RapidHashSet<String>>,
    ) -> Result<Document> {
        context
            .mirror(self.prepare_write(
                collection_name,
                doc,
                modified_fields,
                context.encryption().cloned(),
            ))
            .await
    }

    /// [`Self::prepare_write_as`] for every document one request creates in
    /// `collection_name`, in order. A mutator whose inputs batch across
    /// documents overrides it.
    async fn prepare_creates_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> Result<Vec<Document>> {
        prepare_each(self, context, collection_name, docs).await
    }

    /// Resolve one exact external request after a transaction attempt has
    /// ended. Implementations parse the request according to `kind` and return
    /// the provider response bytes for per-request memoization.
    async fn resolve_prepared_input(
        &self,
        _request: &crate::prepared::PreparedInputRequest,
    ) -> Result<Arc<[u8]>> {
        Err(crate::error::QueryError::execution(
            "prepared input resolution is not supported by this mutator",
        ))
    }

    /// [`Self::resolve_prepared_input`] for the request `context` serves.
    async fn resolve_prepared_input_as(
        &self,
        context: &RequestContext,
        request: &crate::prepared::PreparedInputRequest,
    ) -> Result<Arc<[u8]>> {
        context.mirror(self.resolve_prepared_input(request)).await
    }

    /// Resolve a wave of exact external requests at once, one answer per
    /// request in order. The default resolves each with
    /// [`resolve_prepared_input`](Self::resolve_prepared_input),
    /// concurrently; a mutator whose inputs batch overrides it.
    async fn resolve_prepared_inputs(
        &self,
        requests: Vec<crate::prepared::PreparedInputRequest>,
    ) -> Result<Vec<Arc<[u8]>>> {
        futures::future::try_join_all(
            requests
                .iter()
                .map(|request| self.resolve_prepared_input(request)),
        )
        .await
    }

    /// [`Self::resolve_prepared_inputs`] for the request `context` serves.
    /// The default resolves each with
    /// [`resolve_prepared_input_as`](Self::resolve_prepared_input_as),
    /// concurrently, as the plain default composes the plain one; a mutator
    /// whose inputs batch overrides both forms.
    async fn resolve_prepared_inputs_as(
        &self,
        context: &RequestContext,
        requests: Vec<crate::prepared::PreparedInputRequest>,
    ) -> Result<Vec<Arc<[u8]>>> {
        futures::future::try_join_all(
            requests
                .iter()
                .map(|request| self.resolve_prepared_input_as(context, request)),
        )
        .await
    }

    /// Build the exact remote request for a regenerated embedding using the
    /// current attempt's source values. This method is pure local work.
    async fn prepared_embedding_request(
        &self,
        _collection_name: &str,
        _field_name: &str,
        _doc: &Document,
    ) -> Result<Option<crate::prepared::PreparedInputRequest>> {
        Ok(None)
    }

    /// Configure document ACP for mutation-side ordering guarantees.
    fn set_document_acp(&self, _acp: Arc<dyn acp::DocumentACP>) {}

    /// Whether the concrete mutator stages external ACP create/delete hooks
    /// inside the storage attempt that owns each document write.
    fn manages_external_acp_hooks(&self) -> bool {
        false
    }

    /// Attach a fallible external side effect to this mutation's durable commit.
    ///
    /// Implementations must retain the hook only for the winning transaction
    /// attempt and run it before ordered events become visible. Providers that
    /// cannot make this guarantee must return an error.
    async fn register_ordered_async_hook(
        &self,
        _callback: storage::corekv::CommitHook,
    ) -> Result<()> {
        Err(crate::error::QueryError::execution(
            "ordered post-commit hooks are unsupported by this mutator",
        ))
    }

    /// Begin a request-scoped mutation batch.
    ///
    /// Implementations can override this to provide a shared transaction for
    /// multiple top-level GraphQL mutations in one request. The default
    /// implementation disables batching.
    async fn begin_batch(&self) -> Result<Option<MutationBatch>> {
        Ok(None)
    }

    /// Create a new document in a collection.
    ///
    /// The document should NOT have an ID set - the mutator will generate
    /// a content-based DocID and set it on the document before persisting.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - The name of the collection to create the document in
    /// * `doc` - The document to create (ID will be generated)
    ///
    /// # Returns
    ///
    /// The generated DocID and the document with ID set.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The collection does not exist
    /// - The document fails schema validation
    /// - A document with the same content already exists
    async fn create(&self, collection_name: &str, doc: Document) -> Result<CreateResult>;

    /// [`Self::create`] for the request `context` serves.
    async fn create_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc: Document,
    ) -> Result<CreateResult> {
        context.mirror(self.create(collection_name, doc)).await
    }

    /// Create multiple documents in a single transaction.
    ///
    /// The default implementation calls `create()` in a loop. Implementations
    /// can override for single-transaction batching (one commit/fsync for N docs).
    async fn create_many(
        &self,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> Result<Vec<CreateResult>> {
        let mut results = Vec::with_capacity(docs.len());
        for doc in docs {
            results.push(self.create(collection_name, doc).await?);
        }
        Ok(results)
    }

    /// [`Self::create_many`] for the request `context` serves.
    async fn create_many_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> Result<Vec<CreateResult>> {
        let mut results = Vec::with_capacity(docs.len());
        for doc in docs {
            results.push(self.create_as(context, collection_name, doc).await?);
        }
        Ok(results)
    }

    /// Update an existing document.
    ///
    /// The document must have a valid DocID set. Only dirty (modified) fields
    /// will be updated in storage.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - The name of the collection
    /// * `doc` - The document with updated fields
    /// * `modified_fields` - The set of field names that were modified
    ///
    /// # Returns
    ///
    /// The updated document and count of modified fields.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The collection does not exist
    /// - The document does not have an ID
    /// - The document does not exist in the collection
    /// - The document fails schema validation
    async fn update(
        &self,
        collection_name: &str,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> Result<UpdateResult>;

    /// [`Self::update`] for the request `context` serves.
    async fn update_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> Result<UpdateResult> {
        context
            .mirror(self.update(collection_name, doc, modified_fields))
            .await
    }

    /// Update a document only if its persisted state still matches the
    /// snapshot used to select and materialize the mutation.
    ///
    /// Transaction-scoped mutators already read and write through one
    /// snapshot, so the default delegates to [`Self::update`]. Auto-commit
    /// mutators should override this method and validate `expected` after
    /// acquiring their per-document serialization guard.
    async fn update_if_unchanged(
        &self,
        collection_name: &str,
        expected: Document,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> Result<UpdateResult> {
        let _ = expected;
        self.update(collection_name, doc, modified_fields).await
    }

    /// [`Self::update_if_unchanged`] for the request `context` serves.
    async fn update_if_unchanged_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        expected: Document,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> Result<UpdateResult> {
        let _ = expected;
        self.update_as(context, collection_name, doc, modified_fields)
            .await
    }

    /// Update only if the named fields in the previously selected document
    /// still match. Implementations without projected-write support retain
    /// the conservative full-document check.
    async fn update_if_fields_unchanged(
        &self,
        collection_name: &str,
        expected: Document,
        expected_fields: RapidHashSet<String>,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> Result<UpdateResult> {
        let _ = expected_fields;
        self.update_if_unchanged(collection_name, expected, doc, modified_fields)
            .await
    }

    /// [`Self::update_if_fields_unchanged`] for the request `context` serves.
    async fn update_if_fields_unchanged_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        expected: Document,
        expected_fields: RapidHashSet<String>,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> Result<UpdateResult> {
        let _ = expected_fields;
        self.update_if_unchanged_as(context, collection_name, expected, doc, modified_fields)
            .await
    }

    /// Delete a document by ID.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - The name of the collection
    /// * `doc_id` - The ID of the document to delete
    ///
    /// # Returns
    ///
    /// Whether the document existed before deletion.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The collection does not exist
    /// - A storage error occurs
    async fn delete(&self, collection_name: &str, doc_id: &DocID) -> Result<DeleteResult>;

    /// [`Self::delete`] for the request `context` serves.
    async fn delete_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc_id: &DocID,
    ) -> Result<DeleteResult> {
        context.mirror(self.delete(collection_name, doc_id)).await
    }

    /// Check if a document exists.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - The name of the collection
    /// * `doc_id` - The ID of the document to check
    ///
    /// # Returns
    ///
    /// `true` if the document exists, `false` otherwise.
    async fn exists(&self, collection_name: &str, doc_id: &DocID) -> Result<bool>;

    /// Get a document by ID for updating.
    ///
    /// This is a convenience method for fetching a document before updating it.
    /// The returned document will have dirty tracking reset.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - The name of the collection
    /// * `doc_id` - The ID of the document to fetch
    ///
    /// # Returns
    ///
    /// The document if it exists, `None` otherwise.
    async fn get_for_update(
        &self,
        collection_name: &str,
        doc_id: &DocID,
    ) -> Result<Option<Document>>;
}

/// Prepares each of `docs` for creation with `mutator`, one after another.
pub async fn prepare_each<M>(
    mutator: &M,
    context: &RequestContext,
    collection_name: &str,
    docs: Vec<Document>,
) -> Result<Vec<Document>>
where
    M: DocMutator + ?Sized,
{
    let mut prepared = Vec::with_capacity(docs.len());
    for doc in docs {
        prepared.push(
            mutator
                .prepare_write_as(context, collection_name, doc, None)
                .await?,
        );
    }
    Ok(prepared)
}
