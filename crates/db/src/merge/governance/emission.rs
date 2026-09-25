//! What a verdict may write beside itself.
//!
//! A validator judging a composite or a definition may emit documents: facts
//! about bytes it holds, built here as unsigned, unencrypted genesis
//! composites so the same fact is the same CID everywhere it is found. An
//! emitted document is merged through the ordinary path, by way of the
//! re-drive queue: judged if its collection is claimed, its heads installed,
//! forwarded to replicators. Nothing about it is special once it is a block.
//!
//! Emissions are queued while a block is judged and written once its merge
//! attempt returns, never from inside the attempt: the attempt may still fail
//! and be retried, and a retry judges again. An attempt that errors discards
//! what it queued.
//!
//! A record's own judgement may emit in turn. Each emitted document remembers
//! how deep in such a chain it sits, and an emission past
//! [`MAX_EMISSION_DEPTH`] is dropped with a warning: a rule that records its
//! own records would otherwise never stop.

use bytes::Bytes;
use cid::Cid;
use defra_core::merge::MergeBlock;
use document::Document;
use storage::corekv::Store;

use super::validator::Emission;
use crate::block::builder::{compute_document_blocks, DocStorageIdentity};
use crate::merge::merge_handler::{DbMergeHandler, MergeError};

/// How many emissions deep a chain may go: a record of a record of a record
/// of a composite is the last one written.
pub const MAX_EMISSION_DEPTH: usize = 4;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Queue what judging `from` emitted, at one more than `from`'s own depth.
    pub(crate) fn queue_emissions(&self, from: &Cid, emit: Vec<Emission>) {
        if emit.is_empty() {
            return;
        }
        let depth = self
            .emitted_depth
            .lock()
            .unwrap()
            .get(from)
            .copied()
            .unwrap_or(0);
        let mut pending = self.pending_emissions.lock().unwrap();
        pending.extend(emit.into_iter().map(|emission| (depth, emission)));
    }

    /// Forget what the failed attempt queued; its retry judges again.
    pub(crate) fn discard_emissions(&self) {
        self.pending_emissions.lock().unwrap().clear();
    }
}

impl<S: Store + 'static, B: blockstore::Blockstore + 'static> DbMergeHandler<S, B> {
    /// Write everything queued since the last drain, then re-drive so each
    /// record is judged and merged now rather than on the next sweep. Never
    /// fails: a record that cannot be written is logged and dropped, and the
    /// verdict it came with stands.
    pub(crate) async fn write_emissions(&self) {
        let queued = std::mem::take(&mut *self.pending_emissions.lock().unwrap());
        if queued.is_empty() {
            return;
        }
        let mut enqueued = false;
        for (depth, emission) in queued {
            if depth >= MAX_EMISSION_DEPTH {
                tracing::warn!(
                    collection = %emission.collection,
                    depth,
                    "Emission dropped: a chain of records deeper than MAX_EMISSION_DEPTH"
                );
                continue;
            }
            match self.emit_one(depth, emission).await {
                Ok(true) => enqueued = true,
                Ok(false) => {}
                Err(error) => tracing::warn!(%error, "Emission dropped"),
            }
        }
        if enqueued {
            self.redrive_deferred().await;
        }
    }

    /// Build one emission's blocks, hold them, and queue the composite for
    /// re-drive. `Ok(false)` when the record is already held: merged, or
    /// unmerged and the sweep's to re-drive.
    async fn emit_one(&self, depth: usize, emission: Emission) -> Result<bool, MergeError> {
        let mut txn = self.db.new_txn(true).await.map_err(MergeError::Database)?;
        let Some(collection) = txn
            .get_collection(&emission.collection)
            .await
            .map_err(MergeError::Database)?
        else {
            return Err(MergeError::MergeFailed(format!(
                "emission into unknown collection {}",
                emission.collection
            )));
        };
        let version_id = collection.schema().version_id.clone();
        drop(txn);

        let mut doc = Document::new();
        for (name, value) in emission.fields {
            doc.set(name, value);
        }
        // The identity only keys headstore entries, which the merge path
        // writes itself, and scopes encryption, which is off: no node-local
        // value reaches the bytes, which is what makes the record the same
        // CID on every replica.
        let blocks =
            compute_document_blocks(&doc, &version_id, DocStorageIdentity::new(0, 0), None, None)
                .map_err(MergeError::MergeFailed)?;
        let cid = blocks.block_result.cid;

        if self.has_merged_composite(&cid) {
            return Ok(false);
        }
        if self
            .blockstore
            .get(&cid)
            .await
            .map_err(|error| MergeError::Storage(error.to_string()))?
            .is_some()
        {
            return Ok(false);
        }

        self.emitted_depth.lock().unwrap().insert(cid, depth + 1);
        for (key, data) in &blocks.blockstore_entries {
            let block_cid = Cid::try_from(key.as_slice())
                .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
            self.blockstore
                .put(&block_cid, data)
                .await
                .map_err(|error| MergeError::Storage(error.to_string()))?;
        }
        tracing::debug!(%cid, collection = %emission.collection, depth, "Emitted a record");

        // No carrier: like a swept block, it names no document, no
        // collection and no sender, and the merge path resolves all three
        // from the block.
        Ok(self.deferred.enqueue_ready(MergeBlock {
            cid,
            block_data: Bytes::new(),
            doc_id: String::new(),
            collection_id: String::new(),
            creator: String::new(),
            sender_peer: None,
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        }))
    }
}
