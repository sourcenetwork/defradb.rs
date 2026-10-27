//! The one create pipeline. Every create, whoever owns its transaction, forms
//! each document fully in memory and computes every block before anything is
//! stored: [`embed`], then [`prepare_keys`] and [`compute`] (or
//! [`compute_many`]), then [`persist`] inside the transaction. The mutators
//! only own the transaction, node access, ACP registration, events and commit.

use std::sync::Arc;

use bytes::Bytes;
use cid::Cid;
use datastore::NamespaceView;
use defra_core::RequestContext;
use document::{DocID, Document};
use query::prepared::PreparedInputMemo;
use storage::corekv::Store;

use crate::block::builder::{
    compute_document_blocks_with_inputs, insert_computed_blocks, ComputedBlocks, DocRef,
};
use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;
use crate::write::autocommit::helpers::{
    register_created_doc, write_branchable_collection_block, write_local_create,
};
use crate::write::prepare::{prepared_identity, PreparedCreateBlocks};

#[cfg(feature = "native")]
mod runs;

/// The transaction's views a create writes through.
pub(crate) struct CreateStores<'a> {
    pub datastore: &'a NamespaceView,
    pub systemstore: &'a NamespaceView,
    pub blockstore: &'a NamespaceView,
    pub headstore: &'a NamespaceView,
}

/// A stored create: the document under its derived ID, its composite block
/// and, for a branchable collection, the collection block.
pub(crate) struct Created {
    pub doc_id: DocID,
    pub doc: Document,
    pub cid: Cid,
    pub block: Bytes,
    pub collection_block: Option<(Cid, Bytes)>,
}

/// Generate the document's embeddings: from `inputs` when the caller keeps a
/// memo of prepared inputs (a miss is a `NeedsInput`), fetched otherwise.
pub(crate) async fn embed<S>(
    db: &DB<S>,
    collection: &Collection,
    doc: &mut Document,
    inputs: Option<&PreparedInputMemo>,
) -> query::error::Result<()>
where
    S: Store + 'static,
{
    let config = db.options().embedding_config();
    let embeddings = &collection.schema().vector_embeddings;
    match inputs {
        Some(inputs) => crate::search::embedding::set_embedding_from_inputs(
            embeddings,
            doc,
            true,
            &rapidhash::RapidHashSet::default(),
            &config,
            inputs,
        )
        .map(|_| ()),
        None => crate::search::embedding::set_embedding_with(
            db.remote_inputs().embeddings(),
            embeddings,
            doc,
            true,
            None,
            &config,
        )
        .await
        .map(|_| ())
        .map_err(|error| query::error::QueryError::execution(format!("embedding error: {error}"))),
    }
}

/// Give the document its storage identity without minting any key: with a
/// KMS configured, the block builder then asks the memo for each key.
pub(crate) fn seed_identity<S>(db: &DB<S>, doc: &mut Document, identity: DocRef)
where
    S: Store + 'static,
{
    if doc.write_preparation().is_some() {
        return;
    }
    let mut preparation =
        document::WritePreparation::new(identity.collection_short_id, identity.doc_short_id);
    preparation.set_requires_kms_keys(db.kms().is_some());
    doc.set_write_preparation(Arc::new(preparation));
}

/// The document's identity: the one its preparation names, else a fresh short
/// ID in `collection`.
pub(crate) async fn identity<S>(
    db: &DB<S>,
    collection: &Collection,
    doc: &Document,
) -> query::error::Result<DocRef>
where
    S: Store + 'static,
{
    if let Some(identity) = prepared_identity(doc) {
        return Ok(identity);
    }
    let short_id = db
        .next_doc_short_id()
        .await
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    Ok(DocRef::new(collection.resolved_root_id(), short_id))
}

/// The storage identity of an attempt's `ordinal`-th create. The short ID is
/// drawn once and kept in the memo, so every later run of the same statement
/// or attempt gives that create the same ID, and the KMS keys asked for under
/// it are found.
pub(crate) async fn memo_identity<S>(
    db: &DB<S>,
    collection: &Collection,
    memo: &PreparedInputMemo,
    ordinal: u64,
) -> query::error::Result<DocRef>
where
    S: Store + 'static,
{
    let request = query::prepared::PreparedInputRequest::new(
        query::prepared::PreparedInputKind::Allocation,
        [b"doc-short-id-v1".as_slice(), &ordinal.to_be_bytes()].concat(),
    );
    let drawn = match memo.get(&request) {
        Some(drawn) => drawn,
        None => {
            let short_id = db
                .next_doc_short_id()
                .await
                .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
            memo.insert(request, short_id.to_be_bytes().as_slice())
        }
    };
    let short_id = <[u8; 8]>::try_from(drawn.as_ref())
        .map(u64::from_be_bytes)
        .map_err(|_| query::error::QueryError::execution("invalid prepared document short ID"))?;
    Ok(DocRef::new(collection.resolved_root_id(), short_id))
}

/// Mint the document's encryption keys through the KMS, unless a preparation
/// already carries them (or names the memo as their source).
pub(crate) async fn prepare_keys<S>(
    db: &DB<S>,
    doc: &mut Document,
    identity: DocRef,
    context: &RequestContext,
) -> query::error::Result<()>
where
    S: Store + 'static,
{
    if prepared_identity(doc).is_some() {
        return Ok(());
    }
    crate::write::prepare::prepare_keys(db, doc, identity, None, context.encryption()).await
}

/// Every block of one document, computed without touching storage. Blocks
/// prepared before the request reached the database are reused while they
/// name this identity and the schema version they were built against is
/// still current.
pub(crate) fn compute(
    collection: &Collection,
    doc: &Document,
    identity: DocRef,
    context: &RequestContext,
    inputs: Option<&PreparedInputMemo>,
) -> query::error::Result<ComputedBlocks> {
    if let Some(prepared) = doc
        .write_preparation()
        .and_then(|write| write.payload::<PreparedCreateBlocks>())
        .filter(|prepared| {
            prepared.collection_version_id == collection.version_id()
                && prepared.computed.doc_short_id == identity.doc_short_id
        })
    {
        return Ok(prepared.computed.clone());
    }
    compute_document_blocks_with_inputs(
        collection.crdts(),
        doc,
        collection.schema(),
        identity,
        context.encryption(),
        context.signing(),
        inputs,
    )
    .map_err(|error| error.into_query_error_in("failed to compute document blocks"))
}

/// Every document's blocks, in order: spread over a pool's workers and this
/// task on native, sequential on WASM. Every document's missing inputs join
/// one wave; any other failure aborts the batch, since without blocks there is
/// no derived DocID.
pub(crate) async fn compute_many<S>(
    db: &DB<S>,
    collection: &Collection,
    docs: Vec<(Document, DocRef)>,
    context: &RequestContext,
    inputs: Option<&PreparedInputMemo>,
) -> query::error::Result<Vec<(Document, DocRef, ComputedBlocks)>>
where
    S: Store + 'static,
{
    let identities: Vec<_> = docs.iter().map(|(_, identity)| *identity).collect();
    #[cfg(feature = "native")]
    let computed = {
        // The database's pool, else the process's default pool; on a worker
        // before the default pool has started (starting it would block the
        // worker), all here.
        let pool = match db.pool_or_default() {
            Ok(pool) => Some(pool),
            Err(workers::Error::OnWorker) => None,
            Err(error) => {
                return Err(query::error::QueryError::execution(format!(
                    "no worker pool can compute the document blocks: {error}"
                )));
            }
        };
        runs::compute_blocks(
            pool.as_ref(),
            db.crdts(),
            docs,
            &collection.shared_schema(),
            context,
            inputs,
        )
        .await?
    };
    #[cfg(not(feature = "native"))]
    let computed: Vec<_> = {
        let _ = db;
        docs.into_iter()
            .map(|(doc, identity)| {
                let blocks = compute_document_blocks_with_inputs(
                    &doc,
                    collection.schema(),
                    identity,
                    context.encryption(),
                    context.signing(),
                    inputs,
                );
                (doc, blocks)
            })
            .collect()
    };
    let mut first_miss = None;
    let mut out = Vec::with_capacity(computed.len());
    for ((doc, blocks), identity) in computed.into_iter().zip(identities) {
        match blocks {
            Ok(blocks) => out.push((doc, identity, blocks)),
            Err(crate::block::builder::BuildError::NeedsInput(request)) => {
                first_miss.get_or_insert(request);
            }
            Err(error) => {
                return Err(error.into_query_error_in("failed to compute document blocks"))
            }
        }
    }
    match first_miss {
        Some(request) => Err(query::error::QueryError::NeedsInput(request)),
        None => Ok(out),
    }
}

/// Store a computed document inside the caller's transaction: validate it,
/// insert its blocks and heads, map its DocID, write its local document and
/// indexes, and append the branchable collection block.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn persist<S>(
    db: &DB<S>,
    stores: &CreateStores<'_>,
    collection_name: &str,
    collection: &Collection,
    index_manager: &IndexManager,
    doc: &Document,
    computed: &ComputedBlocks,
    context: &RequestContext,
    inputs: Option<&PreparedInputMemo>,
) -> query::error::Result<Created>
where
    S: Store + 'static,
{
    db.validate_downsample_write(stores.datastore, stores.systemstore, collection, doc, None)
        .await
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    insert_computed_blocks(
        stores.blockstore,
        stores.headstore,
        computed,
        context.batch_session(),
    )
    .await
    .map_err(|error| {
        query::error::QueryError::execution(format!(
            "failed to write document blocks for create on collection {collection_name}: {error}"
        ))
    })?;
    let block_result = &computed.block_result;
    let doc_id = register_created_doc(
        db.document_acp(),
        context.creator(),
        stores.systemstore,
        stores.datastore,
        collection,
        computed.doc_short_id,
        block_result,
    )
    .await?;
    // The source document keeps its ID unset through validation; series and
    // index persistence need the derived one.
    let mut doc = doc.clone();
    doc.set_id(doc_id.clone());
    write_local_create(
        stores.datastore,
        collection,
        &doc,
        computed.doc_short_id,
        index_manager,
    )
    .await?;
    let collection_block = write_branchable_collection_block(
        db,
        collection_name,
        collection,
        stores.blockstore,
        stores.headstore,
        block_result.cid,
        computed.doc_short_id,
        context,
        inputs,
    )
    .await?;
    Ok(Created {
        doc_id,
        doc,
        cid: block_result.cid,
        block: block_result.block.clone(),
        collection_block,
    })
}

impl Created {
    /// The mutator's result: the document without its write preparation, the
    /// commit's blocks attached.
    pub(crate) fn into_result(mut self) -> query::mutator::CreateResult {
        self.doc.clear_write_preparation();
        let mut result =
            query::mutator::CreateResult::with_commit(self.doc_id, self.doc, self.cid, self.block);
        if let Some((cid, block)) = self.collection_block {
            result.broadcast_cid = Some(cid);
            result.broadcast_block = Some(block);
        }
        result
    }
}
