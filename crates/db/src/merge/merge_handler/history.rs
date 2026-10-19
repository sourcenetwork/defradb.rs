//! Disk-paged DFS within the caller's engine-owned merge attempt.

use super::batch::{PendingFieldBlockFinalization, PendingMergeEvent, PendingPostCommitAction};
use super::composite::{CompositeMergeMode, CompositeMergePreparation};
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use storage::corekv::IterOptions;

#[path = "history_registry.rs"]
mod registry;

#[path = "history_keys.rs"]
mod keys;

#[path = "history_walk.rs"]
mod walk;

#[cfg(test)]
#[path = "../../../tests/merge/history_cursor.rs"]
mod tests;

const MAX_ROOTS: usize = 64;
const MAX_ROOTS_PER_PEER: usize = 16;
const PAGE_STEPS: usize = 256;

pub(super) fn state_key(cid: &Cid) -> Vec<u8> {
    format!("/merge-history/v1/{cid}/state").into_bytes()
}

#[derive(Default, Serialize, Deserialize)]
struct Progress {
    top: u64,
    nodes: u64,
    doc: Option<String>,
    outcome: Option<Completion>,
    #[serde(default)]
    context: [u8; 32],
    #[serde(default)]
    generation: [u8; 16],
    #[serde(default)]
    last_active_unix: u64,
    #[serde(default)]
    sender_peer: Option<String>,
}

fn now_unix() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn context_key(metadata: &BlockMetadata<'_>) -> Result<[u8; 32], MergeError> {
    let authorization = metadata.explicit_replay_authorization.as_ref().map(|auth| {
        (
            &auth.source_peer_id,
            &auth.target_peer_id,
            &auth.collection_id,
            &auth.authorizer_did,
            auth.expires_at,
            &auth.capability,
        )
    });
    let bytes = serde_json::to_vec(&(
        metadata.collection_id,
        metadata.creator,
        &metadata.verified_creator,
        metadata.is_recovery,
        metadata.is_explicit_replicator,
        metadata.is_schema_block,
        authorization,
    ))
    .map_err(storage_error)?;
    Ok(Sha256::digest(bytes).into())
}

#[derive(Clone, Serialize, Deserialize)]
struct Frame {
    cid: String,
    next: usize,
}

fn storage_error(error: impl std::fmt::Display) -> MergeError {
    MergeError::Storage(error.to_string())
}

async fn read<T>(store: &NamespaceView, key: &[u8]) -> Result<Option<T>, MergeError>
where
    T: serde::de::DeserializeOwned,
{
    store
        .get(key)
        .await
        .map_err(storage_error)?
        .map(|bytes| serde_json::from_slice(&bytes).map_err(storage_error))
        .transpose()
}

async fn write<T>(store: &NamespaceView, key: &[u8], value: &T) -> Result<(), MergeError>
where
    T: Serialize,
{
    store
        .set(key, &serde_json::to_vec(value).map_err(storage_error)?)
        .await
        .map_err(storage_error)
}

#[derive(Clone, Serialize, Deserialize)]
enum Completion {
    Merged,
    Skipped(String),
    Rejected(String),
    Deferred(String),
    Restart,
}

impl Completion {
    fn from_outcome(outcome: MergeOutcome) -> Option<Self> {
        match outcome {
            MergeOutcome::Merged => Some(Self::Merged),
            MergeOutcome::Rejected { reason } => Some(Self::Rejected(reason)),
            MergeOutcome::Skipped {
                reason,
                terminal: true,
            } => Some(Self::Skipped(reason)),
            _ => None,
        }
    }

    fn into_outcome(self) -> MergeOutcome {
        match self {
            Self::Merged => MergeOutcome::Merged,
            Self::Skipped(reason) => MergeOutcome::terminal_skip(reason),
            Self::Rejected(reason) => MergeOutcome::rejected(reason),
            Self::Deferred(reason) => MergeOutcome::retryable_skip(reason),
            Self::Restart => MergeOutcome::Yielded,
        }
    }
}

/// Delete only traversal bookkeeping, never document or block data.
async fn cleanup(store: &NamespaceView, root: &Cid, budget: usize) -> Result<bool, MergeError> {
    let mut iter = store
        .iterator(
            IterOptions::new()
                .with_prefix(format!("/merge-history/v1/{root}/data/").into_bytes())
                .with_validation(true),
        )
        .await
        .map_err(storage_error)?;
    let mut keys = Vec::with_capacity(budget);
    for _ in 0..budget {
        let Some(entry) = iter.next().await.map_err(storage_error)? else {
            break;
        };
        keys.push(entry.key);
    }
    let more = iter.next().await.map_err(storage_error)?.is_some();
    iter.close().await.map_err(storage_error)?;
    for key in keys {
        store.delete(&key).await.map_err(storage_error)?;
    }
    if !more {
        store
            .delete(&state_key(root))
            .await
            .map_err(storage_error)?;
        store
            .delete(&registry::admission_key(root))
            .await
            .map_err(storage_error)?;
    }
    Ok(more)
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    pub(super) async fn composite_was_applied(
        systemstore: &NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        payload: &defra_core::block::CompositeDeltaPayload,
        doc: &str,
    ) -> Result<bool, MergeError> {
        let Some(doc_ref) = crate::docid::map::get_doc_ref(systemstore, doc).await? else {
            return Ok(false);
        };
        crate::block::heads::document::is_published(
            headstore,
            doc_ref.doc_short_id,
            payload.priority,
            cid,
        )
        .await
        .map_err(storage_error)
    }

    /// Runs only after the rejected data attempt was discarded by the engine.
    pub(super) async fn reject_history(
        &self,
        store: &NamespaceView,
        root: &Cid,
        context: &[u8; 32],
        generation: &[u8; 16],
        reason: &str,
    ) -> Result<MergeOutcome, MergeError> {
        let Some(mut progress) = read::<Progress>(store, &state_key(root)).await? else {
            return Ok(MergeOutcome::rejected(reason));
        };
        if &progress.context != context || &progress.generation != generation {
            return Ok(MergeOutcome::Yielded);
        }
        progress.outcome = Some(Completion::Rejected(reason.to_owned()));
        if cleanup(store, root, self.max_merge_depth.clamp(1, PAGE_STEPS)).await? {
            write(store, &state_key(root), &progress).await?;
            Ok(MergeOutcome::Yielded)
        } else {
            Ok(MergeOutcome::rejected(reason))
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn history_turn(
        &self,
        datastore: &NamespaceView,
        headstore: &NamespaceView,
        store: &NamespaceView,
        root: &Cid,
        block: &Block,
        payload: &defra_core::block::CompositeDeltaPayload,
        metadata: &BlockMetadata<'_>,
        from_collection: bool,
        merged: &CidSet,
        events: &SegQueue<PendingMergeEvent>,
        actions: &SegQueue<PendingPostCommitAction>,
        fields: &SegQueue<PendingFieldBlockFinalization>,
    ) -> Result<MergeOutcome, MergeError> {
        let context = context_key(metadata)?;
        let previous = read::<Progress>(store, &state_key(root)).await?;
        let generation = previous
            .as_ref()
            .map(|progress| progress.generation)
            .unwrap_or_else(rand::random);
        self.history_page(
            datastore,
            headstore,
            store,
            root,
            block,
            payload,
            metadata,
            from_collection,
            merged,
            events,
            actions,
            fields,
            context,
            generation,
            previous,
        )
        .await
        .map_err(|error| {
            if error.disposition() == MergeErrorDisposition::Terminal {
                MergeError::HistoryAborted(Box::new(HistoryRejection {
                    root: *root,
                    context,
                    generation,
                    reason: error.to_string(),
                }))
            } else {
                error
            }
        })
    }
}
