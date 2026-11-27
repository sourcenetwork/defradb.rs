//! Makes a merged parent's chunks match its merged text: a peer's text may come without them.

use cid::Cid;
use datastore::NamespaceView;
use defra_core::RequestContext;
use document::{Document, NormalValue};
use events::Update;
use rapidhash::{RapidHashMap, RapidHashSet};
use storage::corekv::Store;
use storage::keys::doc_id_index::DocRef;
use storage::merge::head_set::Set;
use storage::namespace::Namespace;

use crate::block::builder::{compute_document_blocks, write_document_blocks_with_inputs};
use crate::chunk::hash::distinct;
use crate::chunk::source::{field_heads, set_source, winning_commit};
use crate::chunk::split::{split, Chunk};
use crate::chunk::sync::{chunk_document, remove, ChunkCollection};
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
    /// The chunked fields this merge wrote a commit of, with that commit.
    pub(crate) commits: &'a [(String, Cid)],
    /// The parent's head set after the merge.
    pub(crate) heads: &'a Set,
    /// The merged block carries encryption.
    pub(crate) encrypted: bool,
}

/// Brings a merged parent's chunks in line with its text, with an event for each change.
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
        let Some((_, merging)) = parent.commits.iter().find(|(name, _)| name == field) else {
            continue;
        };
        let value = parent.doc.get(field);
        let text = match value {
            Some(NormalValue::String(text)) => Some(text.as_str()),
            None | Some(NormalValue::Null) => None,
            Some(_) => continue,
        };
        // A text this node can't read decides nothing.
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
        let Some(target) = chunk_collection(
            db,
            datastore,
            systemstore,
            &chunked.collection(&schema.name),
        )?
        else {
            continue;
        };
        // Only the writer's model gives a chunk its vector, so a merge never makes one.
        let builds = target.collection.schema().vector_embeddings.is_empty();
        let chunks = match text {
            Some(text) => {
                let config = crate::chunk::chunk_config(&schema.name, chunked).map_err(failed)?;
                split(text, &config)
            }
            None => Vec::new(),
        };
        let heads = field_heads(parent.heads, field);
        let winner = winning_commit(&blockstore, &heads, value)
            .await
            .map_err(failed)?;
        let stored = target.stored_of(parent.doc_id).await.map_err(failed)?;
        let edits = Edits {
            target: &target,
            blockstore: &blockstore,
            headstore: &headstore,
            parent: parent.doc_id,
            merging,
        };
        events.extend(match winner {
            Some(winner) if heads.len() > 1 && builds => {
                edits.move_to(db, &chunks, &stored, winner).await?
            }
            winner => {
                edits
                    .keep_used(db, &chunks, &stored, winner.filter(|_| builds))
                    .await?
            }
        });
    }
    Ok(events)
}

/// Deletes every chunk of a parent whose delete merged, even ones only this node made.
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
        let Some(target) = chunk_collection(
            db,
            datastore,
            systemstore,
            &chunked.collection(&schema.name),
        )?
        else {
            continue;
        };
        let stored = target.stored_of(parent_id).await.map_err(failed)?;
        let short_ids = stored
            .into_values()
            .flatten()
            .map(|doc| doc.short_id)
            .collect();
        events.extend(remove_all_of(&target, &blockstore, &headstore, short_ids).await?);
    }
    Ok(events)
}

fn chunk_collection<S>(
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

/// One chunked field's changes in a merge.
struct Edits<'a> {
    target: &'a ChunkCollection,
    blockstore: &'a NamespaceView,
    headstore: &'a NamespaceView,
    parent: &'a str,
    merging: &'a Cid,
}

impl Edits<'_> {
    /// With one head, the text keeps the chunks it uses and makes the missing
    /// ones under `source`, the same rule as the writer's.
    async fn keep_used<S>(
        &self,
        db: &DB<S>,
        chunks: &[Chunk],
        stored: &RapidHashMap<String, Vec<crate::chunk::sync::Stored>>,
        source: Option<Cid>,
    ) -> Result<Vec<Update>, MergeError>
    where
        S: Store,
    {
        let used: RapidHashSet<&str> = chunks.iter().map(|chunk| chunk.hash.as_str()).collect();
        let settled = self
            .target
            .settle(self.parent, &used, stored, Some(self.merging))
            .await
            .map_err(failed)?;
        let mut events =
            remove_all_of(self.target, self.blockstore, self.headstore, settled.delete).await?;
        let Some(source) = source else {
            return Ok(events);
        };
        for chunk in distinct(chunks) {
            if !settled.held.contains(&chunk.hash) {
                let doc = self.document(chunk, &source)?;
                events.extend(self.build(db, doc).await?);
            }
        }
        Ok(events)
    }

    /// With several heads, every chunk names the winning commit: the others go
    /// unless they wait for a commit not merged here, and the missing ones are made.
    async fn move_to<S>(
        &self,
        db: &DB<S>,
        chunks: &[Chunk],
        stored: &RapidHashMap<String, Vec<crate::chunk::sync::Stored>>,
        winner: Cid,
    ) -> Result<Vec<Update>, MergeError>
    where
        S: Store,
    {
        let mut wanted = RapidHashMap::default();
        for chunk in distinct(chunks) {
            let doc = self.document(chunk, &winner)?;
            wanted.insert(doc_id_of(&self.target.collection, &doc)?, doc);
        }
        let mut gone = Vec::new();
        for doc in stored.values().flatten() {
            let doc_id = self.target.doc_id(doc.short_id).await.map_err(failed)?;
            if wanted.remove(&doc_id).is_none()
                && !self
                    .target
                    .waits(self.parent, doc.source.as_deref(), Some(self.merging))
                    .await
                    .map_err(failed)?
            {
                gone.push(doc.short_id);
            }
        }
        let mut events = remove_all_of(self.target, self.blockstore, self.headstore, gone).await?;
        for doc in wanted.into_values() {
            events.extend(self.build(db, doc).await?);
        }
        Ok(events)
    }

    fn document(&self, chunk: &Chunk, source: &Cid) -> Result<Document, MergeError> {
        let mut doc = chunk_document(
            &self.target.collection,
            self.parent,
            chunk.text.clone(),
            &chunk.hash,
        )
        .map_err(failed)?;
        set_source(&mut doc, source).map_err(failed)?;
        Ok(doc)
    }

    // Unsigned and unencrypted, the chunk's blocks and DocID are the same on every
    // node that builds it, so a copy that arrives later is the same document.
    async fn build<S>(&self, db: &DB<S>, mut doc: Document) -> Result<Option<Update>, MergeError>
    where
        S: Store,
    {
        let target = self.target;
        let collection = &target.collection;
        let doc_id = doc_id_of(collection, &doc)?;
        // A deleted ID can't be used again; the rules never need one, so this only guards the write.
        if crate::docid::map::get_doc_ref(&target.systemstore, &doc_id)
            .await
            .map_err(MergeError::Database)?
            .is_some()
        {
            tracing::warn!(doc_id, chunks = %collection.name(), "a chunk to build is already mapped here");
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
            self.blockstore,
            self.headstore,
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
        Ok(Some(merged_event(
            collection,
            doc_id.to_string(),
            block.cid,
            block.block,
        )))
    }
}

fn doc_id_of(collection: &Collection, doc: &Document) -> Result<String, MergeError> {
    let probe = DocRef::new(collection.resolved_root_id(), 0);
    Ok(compute_document_blocks(
        collection.crdts(),
        doc,
        collection.schema(),
        probe,
        None,
        None,
    )
    .map_err(failed)?
    .block_result
    .doc_id)
}

async fn remove_all_of(
    target: &ChunkCollection,
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    short_ids: Vec<u64>,
) -> Result<Vec<Update>, MergeError> {
    let mut events = Vec::new();
    for doc_short_id in short_ids {
        let removed = remove(target, blockstore, headstore, doc_short_id)
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
