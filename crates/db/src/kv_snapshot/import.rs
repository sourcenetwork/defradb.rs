//! Importing a KV snapshot by replaying each document's blocks through the
//! merge, as a restore: the document's own history gives it its DocID and
//! its values, this node gives it its short IDs, and the merge keeps its
//! indexes. Go's datastore entries are derived state, so they are not read.

use std::io::Read;
use std::sync::Arc;

use super::go_keys;
use blockstore::Blockstore;
use cid::Cid;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use rapidhash::RapidHashMap;
use storage::corekv::Store;

use super::error::KvSnapshotError;
use super::frame::{read_entries, BLOCKSTORE, DATASTORE, HEADSTORE};
use super::mapping::FieldMapping;

/// What an import did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Imported {
    /// Frames read, not counting the end mark.
    pub frames: usize,
    /// Documents whose heads were merged.
    pub documents: usize,
}

/// One document's heads and blocks, gathered while its frames are read.
#[derive(Default)]
struct Pending {
    doc_short_id: u64,
    heads: Vec<Cid>,
    blocks: RapidHashMap<Cid, Vec<u8>>,
}

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// Import the snapshot `input` holds, as the identity acting in `context`.
    /// With the source's `mapping`, the collection it names must exist here
    /// with every field it names. Each document is merged from its blocks;
    /// one whose history the snapshot does not carry back to its first
    /// version cannot be given its DocID, and is reported after the others
    /// are imported. Importing a snapshot again changes nothing.
    pub async fn import_kv_snapshot_as<R>(
        self: &Arc<Self>,
        context: &defra_core::RequestContext,
        input: R,
        mapping: Option<&FieldMapping>,
    ) -> Result<Imported, KvSnapshotError>
    where
        R: Read,
    {
        self.check_node_access_as(context, acp::nac::NodePermission::DocumentUpdate)
            .await?;
        let collection_id = match mapping {
            Some(mapping) => Some(self.check_mapping(mapping)?),
            None => None,
        };
        let blockstore = Arc::new(blockstore::DefraBlockstore::new(
            self.store().clone(),
            false,
        ));
        let handler = crate::merge::DbMergeHandler::new(Arc::clone(self), Arc::clone(&blockstore));
        let creator = context.identity();

        let mut entries = read_entries(input);
        let mut frames = 0;
        let mut documents = 0;
        let mut without_history = Vec::new();
        let mut pending: Option<Pending> = None;
        let mut slice = workers::Budget::new(16);
        loop {
            let entry = entries.next_entry()?;
            let doc = match &entry {
                Some(entry) => {
                    frames += 1;
                    entry_doc(entry.store, &entry.key)?
                }
                None => None,
            };
            let finished = match (&pending, doc, &entry) {
                (Some(current), Some(doc), _) => current.doc_short_id != doc,
                (Some(_), None, None) => true,
                _ => false,
            };
            if finished {
                slice.spend(1).await;
                if let Some(done) = pending.take() {
                    let doc_short_id = done.doc_short_id;
                    match self
                        .restore_document(
                            &handler,
                            &blockstore,
                            done,
                            collection_id.as_deref(),
                            creator,
                        )
                        .await?
                    {
                        true => documents += 1,
                        false => without_history.push(doc_short_id),
                    }
                }
            }
            let Some(entry) = entry else { break };
            if let Some(doc) = doc {
                pending.get_or_insert_with(|| Pending {
                    doc_short_id: doc,
                    ..Pending::default()
                });
            }
            let current = pending.as_mut().ok_or_else(|| {
                KvSnapshotError::Malformed(
                    "a block frame comes before any document's frames".into(),
                )
            })?;
            match entry.store {
                HEADSTORE => {
                    let (_, field, cid) = go_keys::headstore_head(&entry.key)
                        .ok_or_else(|| malformed("headstore key", &entry.key))?;
                    if field == "C" {
                        current.heads.push(cid);
                    }
                }
                BLOCKSTORE => {
                    let cid = Cid::try_from(entry.key.as_slice()).map_err(|error| {
                        KvSnapshotError::Malformed(format!("block key: {error}"))
                    })?;
                    current.blocks.insert(cid, entry.value);
                }
                _ => {}
            }
        }
        if !without_history.is_empty() {
            return Err(KvSnapshotError::NoHistory {
                doc_short_ids: without_history,
                imported: documents,
            });
        }
        Ok(Imported { frames, documents })
    }

    /// The collection `mapping` names, with every field it names: its ID.
    fn check_mapping(&self, mapping: &FieldMapping) -> Result<String, KvSnapshotError> {
        let collection = self
            .get_collection(&mapping.collection_name)?
            .ok_or_else(|| KvSnapshotError::CollectionNotFound(mapping.collection_name.clone()))?;
        for field in mapping
            .fields
            .iter()
            .filter(|field| !field.field_id.is_empty())
        {
            if !collection
                .schema()
                .fields
                .iter()
                .any(|local| local.id == field.field_id)
            {
                return Err(KvSnapshotError::FieldMissing {
                    collection: mapping.collection_name.clone(),
                    name: field.name.clone(),
                    field_id: field.field_id.clone(),
                });
            }
        }
        Ok(collection.collection_id().to_owned())
    }

    /// Store one document's blocks and merge its composite heads. `false`
    /// when its history does not reach its first version, so its DocID, which
    /// that version names, cannot be known.
    async fn restore_document<B>(
        &self,
        handler: &crate::merge::DbMergeHandler<S, B>,
        blockstore: &B,
        pending: Pending,
        collection_id: Option<&str>,
        creator: Option<&str>,
    ) -> Result<bool, KvSnapshotError>
    where
        B: Blockstore + 'static,
    {
        if pending.heads.is_empty() {
            return Err(KvSnapshotError::NoHeads {
                doc_short_id: pending.doc_short_id,
            });
        }
        let blocks: Vec<(&Cid, &[u8])> = pending
            .blocks
            .iter()
            .map(|(cid, bytes)| (cid, bytes.as_slice()))
            .collect();
        blockstore
            .put_many(&blocks)
            .await
            .map_err(|error| KvSnapshotError::Malformed(format!("storing blocks: {error}")))?;
        if !reaches_genesis(blockstore, &pending).await? {
            return Ok(false);
        }
        for head in &pending.heads {
            let bytes = match pending.blocks.get(head) {
                Some(bytes) => bytes.clone(),
                None => blockstore
                    .get(head)
                    .await
                    .map_err(|error| KvSnapshotError::Malformed(error.to_string()))?
                    .ok_or_else(|| KvSnapshotError::Malformed(format!("head {head} has no block")))?
                    .to_vec(),
            };
            loop {
                let outcome = handler
                    .handle_block(head, &bytes, BlockMetadata::restore(collection_id, creator))
                    .await
                    .map_err(|error| KvSnapshotError::NotMerged {
                        doc_short_id: pending.doc_short_id,
                        reason: error.to_string(),
                    })?;
                match outcome {
                    MergeOutcome::Yielded => continue,
                    MergeOutcome::Merged | MergeOutcome::Skipped { terminal: true, .. } => break,
                    MergeOutcome::Skipped { reason, .. } | MergeOutcome::Rejected { reason } => {
                        return Err(KvSnapshotError::NotMerged {
                            doc_short_id: pending.doc_short_id,
                            reason,
                        })
                    }
                    outcome => {
                        return Err(KvSnapshotError::NotMerged {
                            doc_short_id: pending.doc_short_id,
                            reason: format!("{outcome:?}"),
                        })
                    }
                }
            }
        }
        Ok(true)
    }
}

/// Whether every composite version the heads build on is local, back to a
/// first version (one with no heads).
async fn reaches_genesis<B>(blockstore: &B, pending: &Pending) -> Result<bool, KvSnapshotError>
where
    B: Blockstore,
{
    let mut stack = pending.heads.clone();
    let mut seen = rapidhash::RapidHashSet::default();
    while let Some(cid) = stack.pop() {
        if !seen.insert(cid) {
            continue;
        }
        let bytes = match pending.blocks.get(&cid) {
            Some(bytes) => bytes.clone(),
            None => match blockstore
                .get(&cid)
                .await
                .map_err(|error| KvSnapshotError::Malformed(error.to_string()))?
            {
                Some(bytes) => bytes.to_vec(),
                None => return Ok(false),
            },
        };
        let block = defra_core::block::Block::from_dag_cbor(&bytes)
            .map_err(|error| KvSnapshotError::Malformed(format!("block {cid}: {error}")))?;
        stack.extend(block.heads.iter().flatten().copied());
    }
    Ok(true)
}

/// The document a frame belongs to: `None` for a block, which belongs to the
/// document whose frames came before it.
fn entry_doc(store: u8, key: &[u8]) -> Result<Option<u64>, KvSnapshotError> {
    match store {
        DATASTORE => go_keys::datastore_doc(key)
            .map(Some)
            .ok_or_else(|| malformed("datastore key", key)),
        HEADSTORE => go_keys::headstore_head(key)
            .map(|(doc, _, _)| Some(doc))
            .ok_or_else(|| malformed("headstore key", key)),
        BLOCKSTORE => Ok(None),
        store => Err(KvSnapshotError::Malformed(format!(
            "a frame names store {:?}, not d, h or b",
            store as char
        ))),
    }
}

fn malformed(what: &str, key: &[u8]) -> KvSnapshotError {
    KvSnapshotError::Malformed(format!("{what} {key:02x?}"))
}
