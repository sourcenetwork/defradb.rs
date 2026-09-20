//! Durable, bounded DFS for histories that do not fit the merge fast path.

use super::batch::{PendingFieldBlockFinalization, PendingMergeEvent, PendingPostCommitAction};
use super::composite::{CompositeMergeMode, CompositeMergePreparation};
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use storage::corekv::IterOptions;

const ROOTS_KEY: &[u8] = b"/merge-history/v1/roots";
const MAX_ROOTS: u64 = 64;
// Operators can increase the durable per-root admission quota without changing history validity.
fn max_nodes() -> u64 {
    std::env::var("DEFRA_MERGE_HISTORY_MAX_NODES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(262_144)
}

pub(super) fn state_key(cid: &Cid) -> Vec<u8> {
    format!("/merge-history/v1/{cid}/state").into_bytes()
}

#[derive(Default, Serialize, Deserialize)]
struct Progress {
    top: u64,
    nodes: u64,
    doc: Option<String>,
    // Completion is retained until bounded cleanup has released every key.
    outcome: Option<Completion>,
    #[serde(default)]
    context: [u8; 32],
}

fn context_key(metadata: &BlockMetadata<'_>) -> Result<[u8; 32], MergeError> {
    let authorization = metadata
        .explicit_replay_authorization
        .as_ref()
        .map(|authorization| {
            (
                &authorization.source_peer_id,
                &authorization.target_peer_id,
                &authorization.collection_id,
                &authorization.authorizer_did,
                authorization.expires_at,
                &authorization.capability,
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

#[derive(Serialize, Deserialize)]
struct Frame {
    cid: String,
    next: usize,
}

fn storage_error(error: impl std::fmt::Display) -> MergeError {
    MergeError::Storage(error.to_string())
}

async fn read<T: serde::de::DeserializeOwned>(
    store: &NamespaceView,
    key: &[u8],
) -> Result<Option<T>, MergeError> {
    store
        .get(key)
        .await
        .map_err(storage_error)?
        .map(|bytes| serde_json::from_slice(&bytes).map_err(storage_error))
        .transpose()
}

async fn write<T: Serialize>(
    store: &NamespaceView,
    key: &[u8],
    value: &T,
) -> Result<(), MergeError> {
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
            Self::Restart => MergeOutcome::Yielded,
        }
    }
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    pub(super) async fn resume_composite_history(
        &self,
        root: &Cid,
        block: &Block,
        payload: &defra_core::block::CompositeDeltaPayload,
        metadata: &BlockMetadata<'_>,
        from_collection: bool,
    ) -> Result<MergeOutcome, MergeError> {
        let collection = self
            .block_collection(&payload.schema_version_id, metadata.collection_id)
            .await?;
        let _collection_guard = match collection {
            Some(collection) => Some(
                self.db
                    .collection_read_guard(collection.collection_id())
                    .await?,
            ),
            None => None,
        };
        // Separate from document locks: multiple roots may share one document.
        let _root_guard = self.merge_queue.acquire(&format!("history:{root}")).await;
        let result = self
            .history_turn(root, block, payload, metadata, from_collection)
            .await;
        if let Err(error) = &result {
            if error.disposition() == defra_core::merge::MergeErrorDisposition::Terminal {
                let txn = self.db.new_txn(false).await?;
                let store = txn.systemstore()?;
                if let Some(mut progress) = read::<Progress>(&store, &state_key(root)).await? {
                    progress.outcome = Some(Completion::Rejected(error.to_string()));
                    write(&store, &state_key(root), &progress).await?;
                    drop(store);
                    txn.force_commit().await?;
                    return Ok(MergeOutcome::Yielded);
                }
            }
        }
        result
    }

    async fn history_turn(
        &self,
        root: &Cid,
        root_block: &Block,
        root_payload: &defra_core::block::CompositeDeltaPayload,
        metadata: &BlockMetadata<'_>,
        from_collection: bool,
    ) -> Result<MergeOutcome, MergeError> {
        let budget = self.max_merge_depth.clamp(1, 1024);
        let node_limit = max_nodes();
        let context = context_key(metadata)?;
        let prefix = format!("/merge-history/v1/{root}/data/");
        let stack_key = |index| format!("{prefix}s/{index:020}").into_bytes();
        let node_key = |cid: &Cid| format!("{prefix}n/{cid}").into_bytes();
        let read_txn = self.db.new_txn(true).await?;
        let previous = read::<Progress>(&read_txn.systemstore()?, &state_key(root)).await?;
        read_txn.force_discard()?;
        let _doc_guard = match previous
            .as_ref()
            .and_then(|progress| progress.doc.as_deref())
        {
            Some(doc) => Some(self.merge_queue.acquire(doc).await),
            None => None,
        };
        let txn = self.db.new_txn(false).await?;
        let store = txn.systemstore()?;
        let mut progress = match read::<Progress>(&store, &state_key(root)).await? {
            Some(progress) => progress,
            None => {
                let roots = read::<u64>(&store, ROOTS_KEY).await?.unwrap_or(0);
                if roots >= MAX_ROOTS {
                    return Ok(MergeOutcome::retryable_skip(
                        "history admission quota reached",
                    ));
                }
                write(&store, ROOTS_KEY, &(roots + 1)).await?;
                write(
                    &store,
                    &stack_key(0),
                    &Frame {
                        cid: root.to_string(),
                        next: 0,
                    },
                )
                .await?;
                store
                    .set(&node_key(root), &[0])
                    .await
                    .map_err(storage_error)?;
                Progress {
                    nodes: 1,
                    context,
                    ..Progress::default()
                }
            }
        };

        if progress.context != context {
            // Never reuse eligibility or partial encrypted work under another grant.
            progress.outcome = Some(Completion::Restart);
            progress.context = context;
        }
        if let Some(completion) = progress.outcome.clone() {
            let mut iter = store
                .iterator(IterOptions::new().with_prefix(prefix.into_bytes()))
                .await
                .map_err(storage_error)?;
            let mut keys = Vec::new();
            for _ in 0..budget {
                let Some(entry) = iter.next().await.map_err(storage_error)? else {
                    break;
                };
                keys.push(entry.key);
            }
            let more = iter.next().await.map_err(storage_error)?.is_some();
            drop(iter);
            for key in keys {
                store.delete(&key).await.map_err(storage_error)?;
            }
            if !more {
                store
                    .delete(&state_key(root))
                    .await
                    .map_err(storage_error)?;
                let roots = read::<u64>(&store, ROOTS_KEY).await?.unwrap_or(1);
                write(&store, ROOTS_KEY, &roots.saturating_sub(1)).await?;
            } else {
                write(&store, &state_key(root), &progress).await?;
            }
            drop(store);
            txn.force_commit().await?;
            if !more && matches!(completion, Completion::Merged) {
                self.merged_composites.insert(*root, ());
            }
            return Ok(if more {
                MergeOutcome::Yielded
            } else {
                completion.into_outcome()
            });
        }

        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let merged = cid_set();
        let events: SegQueue<PendingMergeEvent> = SegQueue::new();
        let actions: SegQueue<PendingPostCommitAction> = SegQueue::new();
        let fields: SegQueue<PendingFieldBlockFinalization> = SegQueue::new();
        let mut root_checked = false;
        let mut waiting = None;

        for _ in 0..budget {
            let mut frame: Frame = read(&store, &stack_key(progress.top))
                .await?
                .ok_or_else(|| storage_error("missing history frame"))?;
            let cid: Cid = frame.cid.parse().map_err(storage_error)?;
            let block = if cid == *root {
                root_block.clone()
            } else {
                let bytes = self
                    .blockstore
                    .get(&cid)
                    .await
                    .map_err(storage_error)?
                    .ok_or_else(|| storage_error(format!("missing history block {cid}")))?;
                Block::from_dag_cbor(&bytes).map_err(|e| MergeError::BlockDecode(e.to_string()))?
            };
            let CrdtDelta::Composite(payload) = &block.delta else {
                return Err(MergeError::UnsupportedDelta(
                    "non-composite history ancestor".into(),
                ));
            };
            let heads = block.heads.as_deref().unwrap_or_default();
            if progress.doc.is_none() {
                let owners =
                    crate::docid::map::get_doc_ids_for_block(&store, &cid.to_string()).await?;
                progress.doc = if heads.is_empty() {
                    Some(crate::block::builder::derive_doc_id(&cid))
                } else if owners.len() == 1 {
                    owners.into_iter().next()
                } else {
                    None
                };
            }
            if let Some(doc) = progress.doc.as_deref() {
                if _doc_guard.is_none() {
                    // Identity discovery commits before taking the document lock next turn.
                    break;
                }
                if !root_checked {
                    if let CompositeMergePreparation::Complete(outcome) = self
                        .prepare_composite_merge(
                            root,
                            root_block,
                            root_payload,
                            metadata,
                            doc,
                            CompositeMergeMode::Batch,
                        )
                        .await?
                    {
                        if outcome.is_rejected() || outcome.is_terminal_skip() {
                            progress.outcome = Completion::from_outcome(outcome);
                        } else {
                            waiting = Some(outcome);
                        }
                        break;
                    }
                    root_checked = true;
                }
            }

            if let Some(parent) = heads.get(frame.next) {
                match store.get(&node_key(parent)).await.map_err(storage_error)? {
                    Some(value) if value.as_ref() == [1] => {
                        frame.next += 1;
                        write(&store, &stack_key(progress.top), &frame).await?;
                        continue;
                    }
                    Some(_) => {
                        return Err(MergeError::UnsupportedDelta(
                            "cyclic composite history".into(),
                        ))
                    }
                    None => {}
                }
                if progress.nodes >= node_limit {
                    waiting = Some(MergeOutcome::retryable_skip(
                        "history storage quota reached; increase DEFRA_MERGE_HISTORY_MAX_NODES",
                    ));
                    break;
                }
                frame.next += 1;
                write(&store, &stack_key(progress.top), &frame).await?;
                progress.nodes += 1;
                progress.top += 1;
                write(
                    &store,
                    &stack_key(progress.top),
                    &Frame {
                        cid: parent.to_string(),
                        next: 0,
                    },
                )
                .await?;
                store
                    .set(&node_key(parent), &[0])
                    .await
                    .map_err(storage_error)?;
                continue;
            }

            let doc = progress
                .doc
                .as_deref()
                .ok_or_else(|| storage_error("history has no identity"))?;
            // Carrier metadata authorizes transport/decryption. prepare_composite_merge
            // independently verifies this frame's signer for protected writes.
            let outcome = match self
                .prepare_composite_merge(
                    &cid,
                    &block,
                    payload,
                    metadata,
                    doc,
                    CompositeMergeMode::Batch,
                )
                .await?
            {
                CompositeMergePreparation::Ready(collection) => {
                    let outcome = self
                        .process_composite_delta_in_txn_body(
                            &datastore,
                            &headstore,
                            &store,
                            &cid,
                            &block,
                            payload,
                            metadata,
                            from_collection,
                            cid == *root,
                            &merged,
                            &events,
                            &actions,
                            &fields,
                            doc,
                            collection.map(|collection| *collection),
                            CompositeMergeMode::History,
                        )
                        .await?;
                    if let MergeOutcome::Rejected { reason } = outcome {
                        // Unique rejection may have staged document writes: discard the page.
                        return Err(MergeError::UniqueConstraintViolation(reason));
                    }
                    outcome
                }
                CompositeMergePreparation::Complete(outcome) => outcome,
            };
            if outcome.is_rejected() {
                progress.outcome = Completion::from_outcome(outcome);
                break;
            }
            if !outcome.is_merged() && !outcome.is_terminal_skip() {
                waiting = Some(outcome);
                break;
            }
            store
                .set(&node_key(&cid), &[1])
                .await
                .map_err(storage_error)?;
            store
                .delete(&stack_key(progress.top))
                .await
                .map_err(storage_error)?;
            if progress.top == 0 {
                progress.outcome = Completion::from_outcome(outcome);
                break;
            }
            progress.top -= 1;
        }

        // Applied counter markers, documents, visited nodes and the cursor commit together.
        write(&store, &state_key(root), &progress).await?;
        drop(datastore);
        drop(headstore);
        drop(store);
        txn.force_commit().await?;
        while let Some(action) = actions.pop() {
            if let Err(error) = action.action.run().await {
                tracing::warn!(%error, "History post-commit action failed");
            }
        }
        let mut cids = Vec::new();
        while let Some(field) = fields.pop() {
            cids.extend(field.cids);
        }
        cids.sort_unstable();
        cids.dedup();
        self.best_effort_finalize_linked_field_blocks(&cids).await;
        if let Some(bus) = self.db.event_bus() {
            let mut messages = Vec::new();
            while let Some(event) = events.pop() {
                messages.push(event.message);
            }
            bus.publish_batch(messages);
        }
        Ok(waiting.unwrap_or(MergeOutcome::Yielded))
    }
}
