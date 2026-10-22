//! Field blocks of a custom CRDT type this node does not have. The merge that
//! meets one merges the rest of its composite and parks the block in the same
//! transaction: the block stays in the blockstore, its field's head is not
//! published, and a record names it. A node built with the type merges its
//! parked blocks in a background task, a page at a time, each in the
//! transaction that removes its record, so a replay that stops and runs again
//! merges each block exactly once. Until a block merges, its field reads as
//! the blocks merged so far give it (unset when none has).
use super::*;
use datastore::BasicTxn;
use schema::CrdtName;
use storage::corekv::{IterOptions, RetryAttemptError, RetryAttemptValue};
use storage::keys::headstore::HeadstoreDocKey;
use storage::keys::ParkedCrdtBlockKey;
use storage::namespace::Namespace;

/// The most parked records one scan reads, and so one slice of the replay
/// between yields.
const PAGE: usize = 64;

/// Where a block parks: its collection, document and field.
pub(super) struct Place<'a> {
    pub(super) collection: &'a str,
    pub(super) doc_id: &'a str,
    pub(super) doc_short_id: u64,
    pub(super) field: &'a str,
    pub(super) collection_id: Option<&'a str>,
    pub(super) priority: u64,
}

/// A parked block read and decrypted outside the transaction that merges it.
struct Ready {
    key: Vec<u8>,
    cid: Cid,
    delta: CrdtDelta,
    parents: Vec<Cid>,
    record: Parked,
}

/// What the replay needs of a parked block beyond its key.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Parked {
    cid: String,
    doc_id: String,
    doc_short_id: u64,
    collection_id: Option<String>,
    priority: u64,
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// Record the field block `cid`, of the type `crdt` this node lacks, for
    /// a later build that has the type. A blind put under a key of its own:
    /// parking never conflicts.
    pub(super) async fn park(
        &self,
        datastore: &NamespaceView,
        crdt: CrdtName,
        cid: &Cid,
        place: Place<'_>,
    ) -> std::result::Result<(), MergeError> {
        let record = Parked {
            cid: cid.to_string(),
            doc_id: place.doc_id.to_owned(),
            doc_short_id: place.doc_short_id,
            collection_id: place.collection_id.map(str::to_owned),
            priority: place.priority,
        };
        let value = serde_json::to_vec(&record)
            .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
        let key = ParkedCrdtBlockKey::new(
            crdt.as_str(),
            place.collection,
            place.doc_short_id,
            place.field,
            record.cid.clone(),
        )
        .bytes();
        datastore
            .sibling(Namespace::Systemstore)
            .set(&key, &value)
            .await
            .map_err(|error| MergeError::Storage(error.to_string()))?;
        tracing::warn!(
            %cid,
            %crdt,
            doc_id = place.doc_id,
            "{}; its block is kept and merges once the node registers the type",
            crdt::FieldCrdtError::Unregistered { crdt }
        );
        Ok(())
    }
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + 'static,
{
    /// Merge the parked blocks of the node's custom types in the background,
    /// on the database's pool.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_parked_replay(self) -> workers::Background {
        let pool = self.db.pool().cloned();
        workers::spawn_background(
            pool.as_ref(),
            "db::parked_crdt_replay",
            move || async move {
                match self.replay_parked().await {
                    Ok(0) => {}
                    Ok(merged) => {
                        tracing::info!(merged, "merged parked blocks of custom CRDT types")
                    }
                    Err(error) => tracing::error!(%error, "parked custom CRDT blocks stay parked"),
                }
            },
        )
    }

    /// Merge every parked block of a type the node has, a page at a time,
    /// each page in one transaction, and return how many merged. A block
    /// that cannot merge yet (its decryption key is not local) stays parked
    /// for the next build.
    pub async fn replay_parked(&self) -> std::result::Result<usize, MergeError> {
        let mut merged = 0;
        for crdt in self.db.crdts().custom_names() {
            let mut after = None;
            loop {
                let mut page = self.parked(crdt, after.take()).await?;
                let full = page.len() == PAGE;
                after = page.last().map(|(key, _)| key.clone());
                page.sort_by_key(|(_, record)| record.priority);
                let mut ready = Vec::with_capacity(page.len());
                for (key, record) in page {
                    if let Some(block) = self.ready(crdt, key, record).await? {
                        ready.push(block);
                    }
                }
                merged += self.replay_page(ready).await?;
                if !full {
                    break;
                }
                workers::yield_now().await;
            }
        }
        Ok(merged)
    }

    /// Up to a page of parked records of `crdt`, after the key `after`.
    async fn parked(
        &self,
        crdt: CrdtName,
        after: Option<Vec<u8>>,
    ) -> std::result::Result<Vec<(Vec<u8>, Parked)>, MergeError> {
        let storage = |error: storage::corekv::Error| MergeError::Storage(error.to_string());
        let txn = self.db.new_txn(true).await.map_err(MergeError::Database)?;
        let systemstore = txn.systemstore()?;
        let mut options =
            IterOptions::new().with_prefix(ParkedCrdtBlockKey::crdt_prefix(crdt.as_str()));
        if let Some(mut after) = after {
            after.push(0);
            options = options.with_start(after);
        }
        let mut iter = systemstore.iterator(options).await.map_err(storage)?;
        let mut parked = Vec::with_capacity(PAGE);
        while parked.len() < PAGE {
            let Some(pair) = iter.next().await.map_err(storage)? else {
                break;
            };
            match serde_json::from_slice::<Parked>(&pair.value) {
                Ok(record) => parked.push((pair.key.to_vec(), record)),
                Err(_) => tracing::warn!(%crdt, "ignoring a malformed parked block record"),
            }
        }
        iter.close().await.map_err(storage)?;
        drop(systemstore);
        let _ = txn.discard();
        Ok(parked)
    }

    /// A parked block read and decrypted, ready to merge; `None` when it
    /// stays parked.
    async fn ready(
        &self,
        crdt: CrdtName,
        key: Vec<u8>,
        record: Parked,
    ) -> std::result::Result<Option<Ready>, MergeError> {
        let cid = Cid::try_from(record.cid.as_str())
            .map_err(|error| MergeError::BlockDecode(error.to_string()))?;
        let Some(bytes) = self
            .blockstore
            .get(&cid)
            .await
            .map_err(|error| MergeError::Storage(error.to_string()))?
        else {
            tracing::warn!(%cid, %crdt, "a parked block is missing from the blockstore");
            return Ok(None);
        };
        let block = Block::from_dag_cbor(&bytes)
            .map_err(|error| MergeError::BlockDecode(error.to_string()))?;
        let mut delta = block.delta.clone();
        if block.encryption.is_some() {
            if let Some(data) = delta.field_data_mut() {
                match self
                    .decrypt_block_data(data, block.encryption.as_ref(), None)
                    .await
                {
                    Ok(decrypted) => *data = decrypted,
                    Err(error) => {
                        tracing::warn!(%cid, %crdt, %error, "a parked block stays parked");
                        return Ok(None);
                    }
                }
            }
        }
        Ok(Some(Ready {
            key,
            cid,
            delta,
            parents: block.heads.clone().unwrap_or_default(),
            record,
        }))
    }

    /// Merge a page of ready blocks in one transaction; returns how many
    /// merged.
    async fn replay_page(&self, page: Vec<Ready>) -> std::result::Result<usize, MergeError> {
        if page.is_empty() {
            return Ok(0);
        }
        let handler = self.clone();
        let page = Arc::new(page);
        let attempt = Arc::clone(&page);
        let merged = datastore::transact(
            self.db.store().as_ref(),
            self.db.options().retry_options(),
            move |txn| {
                let (handler, page) = (handler.clone(), Arc::clone(&attempt));
                Box::pin(async move {
                    let mut merged = Vec::with_capacity(page.len());
                    for block in page.iter() {
                        let replayed = handler
                            .replay_in_txn(
                                txn,
                                &block.key,
                                &block.cid,
                                &block.delta,
                                &block.parents,
                                &block.record,
                            )
                            .await
                            .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
                        if replayed {
                            merged.push(block.cid);
                        }
                    }
                    Ok(Box::new(merged) as RetryAttemptValue)
                })
            },
        )
        .await
        .map_err(|error| MergeError::MergeFailed(format!("{error:?}")))?;
        let merged: Vec<Cid> = merged;
        self.best_effort_finalize_linked_field_blocks(&merged).await;
        Ok(merged.len())
    }

    /// The parked block merged as its composite would have merged it: the
    /// field, its head, and the document value and indexes.
    async fn replay_in_txn(
        &self,
        txn: &mut BasicTxn,
        key: &[u8],
        cid: &Cid,
        delta: &CrdtDelta,
        parents: &[Cid],
        record: &Parked,
    ) -> std::result::Result<bool, MergeError> {
        let storage = |error: storage::corekv::Error| MergeError::Storage(error.to_string());
        let mut datastore = txn.datastore();
        let headstore = txn.headstore();
        let systemstore = txn.systemstore();
        if systemstore.get(key).await.map_err(storage)?.is_none() {
            return Ok(false);
        }
        let Some(field_delta) = delta.field() else {
            return Err(MergeError::UnsupportedDelta(
                "a parked block is not a field block".into(),
            ));
        };
        let field_name = field_delta.field_name.to_owned();
        let collection_version_id = field_delta.collection_version_id.to_owned();
        let mut reads = super::block_reads::BlockReads::default();
        let result = self
            .merge_field_delta_in_txn(
                &mut datastore,
                &headstore,
                cid,
                delta,
                record.collection_id.as_deref(),
                &record.doc_id,
                record.doc_short_id,
                Some(parents),
                None,
                &mut reads,
                false,
            )
            .await?;
        if result.parked.is_some() {
            return Ok(false);
        }

        let set = reads
            .heads(&headstore, record.doc_short_id)
            .await
            .map_err(storage)?
            .clone();
        let mut write = storage::merge::head_set::Write::default();
        let head = HeadstoreDocKey::new(record.doc_short_id, &field_name, *cid);
        crate::block::heads::document::publish(
            &headstore,
            &set,
            &mut write,
            &head,
            record.priority,
            parents,
        )
        .await
        .map_err(storage)?;
        crate::block::heads::document::commit(&headstore, record.doc_short_id, &write)
            .await
            .map_err(storage)?;

        let collection = self
            .block_collection(&collection_version_id, record.collection_id.as_deref())
            .await?;
        if let (Some(collection), true) =
            (collection, result.value.is_some() || result.op.is_some())
        {
            let doc_id = DocID::from_string(&record.doc_id)
                .map_err(|error| MergeError::MergeFailed(format!("invalid doc_id: {error}")))?;
            let old = reads
                .document(&collection, &datastore, record.doc_short_id, &doc_id)
                .await
                .map_err(MergeError::Database)?;
            if let Some(old) = old {
                let mut doc = old.clone();
                if let Some(value) = result.value {
                    doc.set_unchecked(field_name.clone(), value);
                }
                if let Some(op) = result.op {
                    doc.edit(field_name.clone(), op)
                        .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
                }
                let priorities: Option<RapidHashMap<_, _>> = result
                    .order
                    .map(|order| std::iter::once((field_name, order)).collect());
                collection
                    .save_with_datastore_priorities(
                        &datastore,
                        &doc,
                        record.doc_short_id,
                        priorities.as_ref(),
                    )
                    .await
                    .map_err(MergeError::Database)?;
                IndexManager::from_collection(collection.resolved_root_id(), collection.schema())
                    .map_err(|error| MergeError::MergeFailed(error.to_string()))?
                    .on_document_update_merge(
                        &datastore,
                        &systemstore,
                        &old,
                        &doc,
                        record.doc_short_id,
                        collection.schema(),
                    )
                    .await
                    .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
            }
        }
        systemstore.delete(key).await.map_err(storage)?;
        Ok(true)
    }
}
