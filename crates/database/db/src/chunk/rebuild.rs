//! Makes a merged parent's chunk documents match its merged text.
//!
//! A merge can keep another peer's text whose chunks never arrive, as when a
//! peer syncs only the parent, so the merge builds them in its own
//! transaction. It also deletes the chunks the merged text no longer uses: no
//! edit of this node may ever do it, as when its own edit lost the merge.
//! `proofs/tla/ChunkMerge.tla` and `proofs/tla/ChunkCleanup.tla` check this.

use datastore::NamespaceView;
use defra_core::RequestContext;
use document::{Document, NormalValue};
use events::Update;
use rapidhash::{RapidHashMap, RapidHashSet};
use storage::corekv::Store;
use storage::keys::doc_id_index::DocRef;
use storage::namespace::Namespace;

use crate::block::builder::{compute_document_blocks, write_document_blocks_with_inputs};
use crate::chunk::generation::{is_taken, next_generation};
use crate::chunk::hash::distinct;
use crate::chunk::split::{split, Chunk};
use crate::chunk::sync::{chunk_document, remove, ChunkCollection, RemoveInputs};
use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;
use crate::merge::merge_handler::MergeError;
use crate::write::autocommit::helpers::{register_created_doc, write_local_create};

fn failed(error: impl std::fmt::Display) -> MergeError {
    MergeError::MergeFailed(format!("chunk rebuild: {error}"))
}

/// What [`reconcile`] needs from the merge.
pub(crate) struct MergedParent<'a> {
    pub(crate) collection: &'a Collection,
    pub(crate) doc_short_id: u64,
    pub(crate) doc_id: &'a str,
    pub(crate) doc: &'a Document,
    pub(crate) merged: &'a RapidHashMap<String, NormalValue>,
    /// The merged block carries encryption.
    pub(crate) encrypted: bool,
}

/// For each merged text, deletes the chunk documents it does not use and
/// builds the ones this node lacks. Returns an update event for each.
pub(crate) async fn reconcile<S>(
    db: &DB<S>,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    parent: MergedParent<'_>,
) -> Result<Vec<Update>, MergeError>
where
    S: Store,
{
    let schema = parent.collection.schema();
    let blockstore = datastore.sibling(Namespace::Blockstore);
    let headstore = datastore.sibling(Namespace::Headstore);
    let mut events = Vec::new();
    for chunked in &schema.chunked_fields {
        let field = chunked.field_name.as_str();
        if !parent.merged.contains_key(field) {
            continue;
        }
        let text = match parent.doc.get(field) {
            Some(NormalValue::String(text)) => Some(text.as_str()),
            None | Some(NormalValue::Null) => None,
            Some(_) => continue,
        };
        // An encrypted chunk takes a key of its own, so its ID differs per
        // node, and a text this node can't read decides nothing.
        if parent.encrypted
            || encrypts(
                &blockstore,
                &headstore,
                parent.collection,
                parent.doc_short_id,
                field,
            )
            .await?
        {
            continue;
        }
        let Some(target) = buildable(
            db,
            datastore,
            systemstore,
            &chunked.collection(&schema.name),
        )?
        else {
            continue;
        };
        let chunks = match text {
            Some(text) => {
                let config = crate::chunk::chunk_config(&schema.name, chunked).map_err(failed)?;
                split(text, &config)
            }
            None => Vec::new(),
        };
        let used: RapidHashSet<&str> = chunks.iter().map(|chunk| chunk.hash.as_str()).collect();
        let stored = target.stored_of(parent.doc_id).await.map_err(failed)?;
        for (hash, short_ids) in &stored {
            let unneeded = target
                .unneeded(used.contains(hash.as_str()), short_ids)
                .await
                .map_err(failed)?;
            events.extend(remove_all_of(&target, &blockstore, &headstore, unneeded).await?);
        }
        for chunk in distinct(&chunks) {
            if !stored.contains_key(&chunk.hash) {
                events
                    .push(build(db, &target, &blockstore, &headstore, parent.doc_id, chunk).await?);
            }
        }
    }
    Ok(events)
}

/// Deletes every chunk document of a parent whose delete merged. This node may
/// have built some under an ID no other node used, which no delete it receives
/// would name.
pub(crate) async fn remove_all<S>(
    db: &DB<S>,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    parent: &Collection,
    parent_id: &str,
) -> Result<Vec<Update>, MergeError>
where
    S: Store,
{
    let schema = parent.schema();
    let blockstore = datastore.sibling(Namespace::Blockstore);
    let headstore = datastore.sibling(Namespace::Headstore);
    let mut events = Vec::new();
    for chunked in &schema.chunked_fields {
        let Some(target) = buildable(
            db,
            datastore,
            systemstore,
            &chunked.collection(&schema.name),
        )?
        else {
            continue;
        };
        let stored = target.stored_of(parent_id).await.map_err(failed)?;
        let short_ids = stored.into_values().flatten().collect();
        events.extend(remove_all_of(&target, &blockstore, &headstore, short_ids).await?);
    }
    Ok(events)
}

// The chunk collection `name` when a merge may build its chunks. Only the
// author's model gives a chunk its vector, so those come from the author with
// the article, as encrypted ones do.
fn buildable<S>(
    db: &DB<S>,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    name: &str,
) -> Result<Option<ChunkCollection>, MergeError>
where
    S: Store,
{
    let Some(chunks_col) = db.get_collection(name).map_err(MergeError::Database)? else {
        return Ok(None);
    };
    if !chunks_col.schema().vector_embeddings.is_empty() {
        return Ok(None);
    }
    let index_manager = IndexManager::from_collection(
        chunks_col.resolved_root_id(),
        chunks_col.schema(),
        chunks_col.crdts(),
    )
    .map_err(failed)?;
    ChunkCollection::new(
        chunks_col,
        datastore.clone(),
        systemstore.clone(),
        index_manager,
    )
    .map(Some)
    .map_err(failed)
}

async fn remove_all_of(
    target: &ChunkCollection,
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    short_ids: Vec<u64>,
) -> Result<Vec<Update>, MergeError> {
    let mut events = Vec::new();
    for doc_short_id in short_ids {
        // Unsigned like the chunk's blocks, so peers that delete it write one block.
        let removed = remove(
            target,
            blockstore,
            headstore,
            doc_short_id,
            RemoveInputs::default(),
        )
        .await
        .map_err(failed)?;
        if let Some(removed) = removed {
            events.push(merged_event(
                &target.collection,
                removed.doc_id.to_string(),
                removed.block.cid,
                removed.block.block,
            ));
        }
    }
    Ok(events)
}

// The same event a merged document gets, so subscribers here see the chunk.
fn merged_event(
    collection: &Collection,
    doc_id: String,
    cid: cid::Cid,
    block: bytes::Bytes,
) -> Update {
    Update::new(
        doc_id,
        cid,
        collection.version_id().to_string(),
        block,
        false,
        true,
    )
    .with_observation_collection_id(collection.collection_id().to_string())
}

async fn encrypts(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    parent: &Collection,
    doc_short_id: u64,
    field: &str,
) -> Result<bool, MergeError> {
    let (encrypt_doc, fields) = crate::block::builder::inherited_encryption_policy(
        blockstore,
        headstore,
        DocRef::new(parent.resolved_root_id(), doc_short_id),
        &[field.to_string()],
    )
    .await
    .map_err(failed)?;
    Ok(encrypt_doc || !fields.is_empty())
}

// Unsigned and unencrypted, the chunk's blocks and DocID are the same on every
// node that builds it, so a copy that arrives later is the same document.
async fn build<S>(
    db: &DB<S>,
    target: &ChunkCollection,
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    parent_id: &str,
    chunk: &Chunk,
) -> Result<Update, MergeError>
where
    S: Store,
{
    let collection = &target.collection;
    let mut doc =
        chunk_document(collection, parent_id, chunk.text.clone(), &chunk.hash).map_err(failed)?;
    let probe = DocRef::new(collection.resolved_root_id(), 0);
    // An ID this node used before, live or deleted, is not built again: the
    // next generation gets a new one, the one every peer picks after that delete.
    loop {
        let doc_id = compute_document_blocks(
            collection.crdts(),
            &doc,
            collection.schema(),
            probe,
            None,
            None,
        )
        .map_err(failed)?
        .block_result
        .doc_id;
        if !is_taken(&target.systemstore, &doc_id)
            .await
            .map_err(failed)?
        {
            break;
        }
        next_generation(&mut doc).map_err(failed)?;
    }
    let identity = DocRef::new(
        collection.resolved_root_id(),
        db.next_doc_short_id().await.map_err(MergeError::Database)?,
    );
    // Unencrypted, the blocks need no keys, so the writer asks the KMS for none.
    doc.set_write_preparation(std::sync::Arc::new(document::WritePreparation::new(
        identity.collection_short_id,
        identity.doc_short_id,
    )));
    let block = write_document_blocks_with_inputs(
        blockstore,
        headstore,
        &doc,
        collection.schema(),
        identity,
        None,
        None,
        None,
        None,
        db.kms().as_ref(),
        None,
        None,
    )
    .await
    .map_err(failed)?;
    let doc_id = register_created_doc(
        None,
        &RequestContext::default(),
        &target.systemstore,
        &target.datastore,
        collection,
        &doc,
        identity.doc_short_id,
        &block,
        Default::default(),
    )
    .await
    .map_err(failed)?;
    doc.set_id(doc_id.clone());
    write_local_create(
        &target.datastore,
        collection,
        &doc,
        identity.doc_short_id,
        &target.index_manager,
    )
    .await
    .map_err(failed)?;
    doc.clear_write_preparation();
    Ok(merged_event(
        collection,
        doc_id.to_string(),
        block.cid,
        block.block,
    ))
}
