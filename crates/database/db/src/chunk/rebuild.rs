//! Builds the chunks of a merged parent's text that this node lacks.
//!
//! A merge can keep another peer's text whose chunks never arrive, as when a
//! peer syncs only the parent. So the merge builds them in its own
//! transaction. `proofs/tla/ChunkMerge.tla` checks this.

use datastore::NamespaceView;
use defra_core::RequestContext;
use document::{Document, NormalValue};
use events::Update;
use rapidhash::{RapidHashMap, RapidHashSet};
use storage::corekv::Store;
use storage::keys::doc_id_index::DocRef;
use storage::namespace::Namespace;

use crate::block::builder::{compute_document_blocks, write_document_blocks_with_inputs};
use crate::chunk::list::distinct;
use crate::chunk::split::{split, Chunk};
use crate::chunk::sync::{chunk_document, ChunkCollection};
use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;
use crate::merge::merge_handler::MergeError;
use crate::write::autocommit::helpers::{register_created_doc, write_local_create};

fn failed(error: impl std::fmt::Display) -> MergeError {
    MergeError::MergeFailed(format!("chunk rebuild: {error}"))
}

/// What [`build_missing`] needs from the merge.
pub(crate) struct MergedParent<'a> {
    pub(crate) collection: &'a Collection,
    pub(crate) doc_short_id: u64,
    pub(crate) doc_id: &'a str,
    pub(crate) doc: &'a Document,
    pub(crate) merged: &'a RapidHashMap<String, NormalValue>,
    /// The merged block carries encryption.
    pub(crate) encrypted: bool,
}

/// Builds the chunks of each merged text that this node has no document for,
/// and returns an update event for each one built.
pub(crate) async fn build_missing<S>(
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
    let mut built = Vec::new();
    for chunked in &schema.chunked_fields {
        let field = chunked.field_name.as_str();
        if !parent.merged.contains_key(field) {
            continue;
        }
        let Some(NormalValue::String(text)) = parent.doc.get(field) else {
            continue;
        };
        // An encrypted chunk takes a key of its own, so its ID differs per node.
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
        let Some(chunks_col) = db
            .get_collection(&chunked.collection(&schema.name))
            .map_err(MergeError::Database)?
        else {
            continue;
        };
        let index_manager = IndexManager::from_collection(
            chunks_col.resolved_root_id(),
            chunks_col.schema(),
            chunks_col.crdts(),
        )
        .map_err(failed)?;
        let target = ChunkCollection::new(
            chunks_col,
            datastore.clone(),
            systemstore.clone(),
            index_manager,
        )
        .map_err(failed)?;
        let config = crate::chunk::chunk_config(&schema.name, chunked).map_err(failed)?;
        let chunks = split(text, &config);
        let planned: RapidHashSet<&str> = chunks.iter().map(|chunk| chunk.hash.as_str()).collect();
        let existing = target
            .existing_of(parent.doc_id, &planned)
            .await
            .map_err(failed)?;
        for chunk in distinct(&chunks) {
            if !existing.contains(&chunk.hash) {
                built.extend(
                    build(db, &target, &blockstore, &headstore, parent.doc_id, chunk).await?,
                );
            }
        }
    }
    Ok(built)
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
) -> Result<Option<Update>, MergeError>
where
    S: Store,
{
    let collection = &target.collection;
    let mut doc =
        chunk_document(collection, parent_id, chunk.text.clone(), &chunk.hash).map_err(failed)?;
    let probe = DocRef::new(collection.resolved_root_id(), 0);
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
    // A chunk this node already holds under that ID, live or deleted, is not built again.
    if crate::docid::map::get_doc_ref(&target.systemstore, &doc_id)
        .await
        .map_err(MergeError::Database)?
        .is_some()
    {
        return Ok(None);
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
    // The same event a merged document gets, so subscribers here see the chunk.
    Ok(Some(
        Update::new(
            doc_id.to_string(),
            block.cid,
            collection.version_id().to_string(),
            block.block,
            false,
            true,
        )
        .with_observation_collection_id(collection.collection_id().to_string()),
    ))
}
