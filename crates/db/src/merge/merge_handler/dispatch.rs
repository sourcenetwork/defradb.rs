use async_trait::async_trait;
use cid::Cid;
use defra_core::block::Block;
use defra_core::merge::{
    BlockMetadata, ExplicitReplayAuthorization, MergeBlock, MergeErrorDisposition, MergeHandler,
    MergeOutcome, RecoveredBlockMetadata,
};
use storage::corekv::{OnCommit, Store};

use super::{cid_set, DbMergeHandler, MergeError};
use crate::merge::governance::{RedrivenMerge, REDRIVE_BUDGET};
use kovan_queue::seg_queue::SegQueue;
use std::sync::Arc;

struct MergeRetryAttempt<S: Store, B: blockstore::Blockstore> {
    handler: DbMergeHandler<S, B>,
    cid: Cid,
    block_data: Vec<u8>,
    doc_id: Option<String>,
    collection_id: Option<String>,
    creator: Option<String>,
    sender_peer: Option<String>,
    is_explicit_replicator: bool,
    authorization: Option<ExplicitReplayAuthorization>,
    is_recovery: bool,
    is_schema_block: bool,
    history_rejection: Option<Box<super::HistoryRejection>>,
    /// Whether an earlier attempt of this retry operation ran; each one
    /// after the first means that one was abandoned to a conflict.
    attempted: bool,
}

impl<S: Store + 'static, B: blockstore::Blockstore + 'static> storage::corekv::RetryAttemptFn
    for MergeRetryAttempt<S, B>
{
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            match self.run_attempt(retry).await {
                Ok(outcome) => Ok(Box::new(outcome) as storage::corekv::RetryAttemptValue),
                Err(error) => Err(storage::corekv::RetryAttemptError::Abort(Box::new(error))),
            }
        })
    }
}

impl<S: Store + 'static, B: blockstore::Blockstore + 'static> MergeRetryAttempt<S, B> {
    async fn run_attempt(
        &mut self,
        retry: &mut dyn storage::corekv::RetryTxn,
    ) -> Result<MergeOutcome, MergeError> {
        self.handler.reset_retry_dek_prefetch_request();
        if std::mem::replace(&mut self.attempted, true) {
            self.handler.db.merge_stats().record_txn_conflict();
        }
        self.handler.reset_attempt_counts();
        let io = retry
            .begin_attempt_io()
            .map_err(|error| MergeError::Database(crate::error::Error::Storage(error)))?;
        let basic_txn =
            datastore::BasicTxn::from_retry_attempt_io(io, 0, self.handler.db.crdts().clone());
        let txn = self.handler.db.new_db_txn(basic_txn);
        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let systemstore = txn.systemstore()?;
        let batch_merged = cid_set();
        let batch_merged_collections = cid_set();
        let pending_events = SegQueue::new();
        let pending_post_commit_actions = SegQueue::new();
        let pending_field_block_finalizations = SegQueue::new();
        let metadata = BlockMetadata {
            doc_id: self.doc_id.as_deref(),
            collection_id: self.collection_id.as_deref(),
            creator: self.creator.as_deref(),
            sender_peer: self.sender_peer.as_deref(),
            is_explicit_replicator: self.is_explicit_replicator,
            explicit_replay_authorization: self.authorization.clone(),
            verified_creator: None,
            is_recovery: self.is_recovery,
            is_schema_block: self.is_schema_block,
        };
        let result = if let Some(rejection) = &self.history_rejection {
            self.handler
                .reject_history(
                    &systemstore,
                    &rejection.root,
                    &rejection.context,
                    &rejection.generation,
                    &rejection.reason,
                )
                .await
        } else {
            self.handler
                .process_block_in_txn(
                    &datastore,
                    &headstore,
                    &systemstore,
                    &self.cid,
                    &self.block_data,
                    &metadata,
                    &batch_merged,
                    &batch_merged_collections,
                    &pending_events,
                    &pending_post_commit_actions,
                    &pending_field_block_finalizations,
                )
                .await
        };
        drop(datastore);
        drop(headstore);
        drop(systemstore);
        let outcome = match result {
            Ok(MergeOutcome::Rejected { reason }) if self.history_rejection.is_none() => {
                let _ = txn.discard();
                return Err(MergeError::RetryRejected(reason));
            }
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = txn.discard();
                return Err(error);
            }
        };

        let handler = self.handler.clone();
        let merged_composites: Vec<_> = batch_merged.keys().collect();
        let merged_collections: Vec<_> = batch_merged_collections.keys().collect();
        let mut events = Vec::with_capacity(pending_events.len());
        while let Some(event) = pending_events.pop() {
            events.push(event.message);
        }
        let mut actions = Vec::new();
        while let Some(action) = pending_post_commit_actions.pop() {
            actions.push(action.action);
        }
        let mut field_cids = Vec::new();
        while let Some(finalization) = pending_field_block_finalizations.pop() {
            field_cids.extend(finalization.cids);
        }
        field_cids.sort_unstable();
        field_cids.dedup();
        let after_commit = async move {
            handler
                .merged_composites
                .extend(merged_composites.iter().map(|cid| (*cid, ())));
            handler
                .merged_collections
                .extend(merged_collections.iter().map(|cid| (*cid, ())));
            for cid in &merged_composites {
                handler.release_merged_composite(cid, None).await;
            }
            for action in actions {
                if let Err(error) = action.run().await {
                    tracing::warn!(error = %error, "Post-commit merge action failed");
                }
            }
            handler
                .best_effort_finalize_linked_field_blocks(&field_cids)
                .await;
        };
        if events.is_empty() {
            // Deferred/schema-only outcomes may have no native writes. Their
            // follow-up work owns its own commits and needs no receipt slot.
            txn.on_commit(OnCommit::Async(Box::new(move || Box::pin(after_commit))))?;
        } else {
            txn.on_commit(OnCommit::OrderedAsync(Box::new(move || {
                Box::pin(async move {
                    after_commit.await;
                    Ok(())
                })
            })))?;
        }
        if !events.is_empty() {
            if let Some(bus) = self.handler.db.event_bus().cloned() {
                txn.on_commit(OnCommit::Ordered(Box::new(move || {
                    bus.publish_batch(events)
                })))?;
            }
        }
        self.handler.count_merges_on_commit(&txn)?;
        let (io, callbacks) = txn
            .into_retry_attempt()
            .map_err(crate::error::Error::Storage)?;
        retry
            .finish_attempt_io(io, callbacks)
            .map_err(|error| MergeError::Database(crate::error::Error::Storage(error)))?;
        Ok(outcome)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static, B: blockstore::Blockstore + 'static> MergeHandler
    for DbMergeHandler<S, B>
{
    type Error = MergeError;

    fn error_disposition(&self, error: &Self::Error) -> MergeErrorDisposition {
        error.disposition()
    }

    async fn validate_authorization(
        &self,
        authorization: Option<&ExplicitReplayAuthorization>,
        block: &MergeBlock,
    ) -> Result<(), Self::Error> {
        self.validate_explicit_replay_authorization(authorization, block)
            .await
    }

    async fn recover_block_metadata(
        &self,
        cid: &Cid,
        block_data: &[u8],
    ) -> Result<Option<RecoveredBlockMetadata>, Self::Error> {
        self.recover_metadata_from_block(cid, block_data).await
    }

    async fn handle_block(
        &self,
        cid: &Cid,
        block_data: &[u8],
        metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, Self::Error> {
        let result = self.merge_with_retries(cid, block_data, metadata).await;
        self.redrive_deferred().await;
        result
    }

    async fn handle_block_batch(
        &self,
        blocks: &[MergeBlock],
    ) -> Vec<Result<MergeOutcome, Self::Error>> {
        let results = self.try_batch_merge_with_split(blocks).await;
        self.redrive_deferred().await;
        results
    }
}

impl<S: Store + 'static, B: blockstore::Blockstore + 'static> DbMergeHandler<S, B> {
    async fn merge_with_retries(
        &self,
        cid: &Cid,
        block_data: &[u8],
        metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, MergeError> {
        let recovered = if metadata.is_recovery {
            self.recover_metadata_from_block(cid, block_data).await?
        } else {
            None
        };
        let doc_id = metadata
            .doc_id
            .map(str::to_owned)
            .or_else(|| recovered.as_ref().map(|value| value.doc_id.clone()));
        let collection_id = metadata
            .collection_id
            .map(str::to_owned)
            .or_else(|| recovered.as_ref().map(|value| value.collection_id.clone()));
        let creator = metadata
            .creator
            .map(str::to_owned)
            .or_else(|| recovered.as_ref().map(|value| value.creator.clone()));
        let mut retry_deks = super::retry_decryption_keys();
        let mut history_mode = false;
        let mut prepare_history_keys = false;
        let mut history_rejection = None;
        let prefetch_requested = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let retry_metadata = BlockMetadata {
            doc_id: doc_id.as_deref(),
            collection_id: collection_id.as_deref(),
            creator: creator.as_deref(),
            sender_peer: metadata.sender_peer,
            is_explicit_replicator: metadata.is_explicit_replicator,
            explicit_replay_authorization: metadata.explicit_replay_authorization.clone(),
            verified_creator: None,
            is_recovery: metadata.is_recovery,
            is_schema_block: metadata.is_schema_block,
        };
        let kms_context = Self::kms_request_context(Some(&retry_metadata));
        if let Ok(block) = Block::from_dag_cbor(block_data) {
            let cids = self.merge_key_cids(&block).await;
            self.prefill_retry_keys(&kms_context, &cids, &retry_deks)
                .await;
        }
        let result = loop {
            if prepare_history_keys && history_rejection.is_none() {
                if let Ok(block) = Block::from_dag_cbor(block_data) {
                    let cids = self.history_key_cids(&block, cid).await?;
                    self.prefill_retry_keys(&kms_context, &cids, &retry_deks)
                        .await;
                }
                prepare_history_keys = false;
            }
            let mut handler = self
                .with_retry_decryption_context(
                    Arc::clone(&retry_deks),
                    Arc::clone(&prefetch_requested),
                )
                .with_attempt_counts();
            handler.history_mode = history_mode;
            let attempt = MergeRetryAttempt {
                handler,
                cid: *cid,
                block_data: block_data.to_vec(),
                doc_id: doc_id.clone(),
                collection_id: collection_id.clone(),
                creator: creator.clone(),
                sender_peer: metadata.sender_peer.map(str::to_owned),
                is_explicit_replicator: metadata.is_explicit_replicator,
                authorization: metadata.explicit_replay_authorization.clone(),
                is_recovery: metadata.is_recovery,
                is_schema_block: metadata.is_schema_block,
                history_rejection: history_rejection.take(),
                attempted: false,
            };
            let result = self
                .db
                .store()
                .transact_retry(self.db.options().retry_options(), Box::new(attempt))
                .await;
            match result {
                Ok(value) => {
                    let outcome = value
                        .downcast::<MergeOutcome>()
                        .map(|outcome| *outcome)
                        .map_err(|_| {
                            MergeError::MergeFailed("invalid merge retry result".into())
                        })?;
                    if matches!(outcome, MergeOutcome::Yielded) && !self.single_history_turn {
                        retry_deks = super::retry_decryption_keys();
                        prepare_history_keys = history_mode;
                        workers::yield_now().await;
                        continue;
                    }
                    break Ok(outcome);
                }
                Err(error) => {
                    match super::exhausted(&self.db, error).downcast_abort::<MergeError>() {
                        Ok(MergeError::HistoryRequired | MergeError::DepthExceeded { .. })
                            if !history_mode =>
                        {
                            history_mode = true;
                            prepare_history_keys = true;
                        }
                        Ok(MergeError::HistoryAborted(rejection)) => {
                            history_rejection = Some(rejection);
                        }
                        Ok(MergeError::NeedsDecryptionKey(enc_cid)) => {
                            if retry_deks.contains_key(&enc_cid) {
                                break Err(super::repeated_key_request(&enc_cid));
                            }
                            let Some(kms) = self.kms() else {
                                break Err(MergeError::Kms(kms::Error::KeyUnavailable));
                            };
                            let resolved = async {
                                let results = kms
                                    .get_keys(&kms_context, std::slice::from_ref(&enc_cid))
                                    .await?;
                                let key = results
                                    .wait_all()
                                    .await?
                                    .remove(&enc_cid)
                                    .ok_or(kms::Error::KeyUnavailable)?;
                                Ok::<_, kms::Error>((enc_cid, key))
                            }
                            .await;
                            match resolved {
                                Ok((cid, key)) => {
                                    retry_deks.insert(cid, zeroize::Zeroizing::new(key));
                                }
                                Err(error) => break Err(MergeError::Kms(error)),
                            }
                        }
                        Ok(error) => break Err(error),
                        Err(error) => {
                            break Err(MergeError::Database(crate::error::Error::Storage(
                                error.into_storage(),
                            )));
                        }
                    }
                }
            }
        };
        // Preserve sync-time warming for encrypted standalone field blocks,
        // but spawn the detached request only after the retry engine has
        // stopped invoking the attempt closure.
        if prefetch_requested.load(std::sync::atomic::Ordering::Acquire) {
            if let Ok(block) = Block::from_dag_cbor(block_data) {
                if let Some(enc_cid) = block.encryption {
                    self.spawn_dek_prefetch(enc_cid, &retry_metadata);
                }
            }
        }
        match result {
            Err(MergeError::RetryRejected(reason)) => Ok(MergeOutcome::rejected(reason)),
            Err(error) if error.disposition() == MergeErrorDisposition::Terminal => {
                Ok(MergeOutcome::rejected(error.to_string()))
            }
            other => other,
        }
    }

    /// Re-merge deferred composites whose awaited CIDs have merged, through
    /// the normal path: signatures are verified and the validator judges each
    /// one again. Bounded by [`REDRIVE_BUDGET`] per call; composites released
    /// by a re-driven merge join the same drain.
    pub async fn redrive_deferred(&self) {
        let mut budget = REDRIVE_BUDGET;
        while budget > 0 {
            let Some(entry) = self.deferred.take_ready() else {
                return;
            };
            budget -= 1;
            let data = match self.blockstore.get(&entry.cid).await {
                Ok(Some(data)) => data,
                Ok(None) => continue,
                Err(error) => {
                    tracing::debug!(cid = %entry.cid, %error, "Deferred composite unreadable for re-drive");
                    continue;
                }
            };
            // A swept composite has no carrier, so it names no creator and no
            // sending peer; an id left empty reads as absent, not as "".
            let mut metadata = BlockMetadata::normal(
                &entry.doc_id,
                &entry.collection_id,
                &entry.creator,
                entry.sender_peer.as_deref(),
                entry.is_explicit_replicator,
            )
            .with_explicit_replay_authorization(entry.explicit_replay_authorization.clone());
            metadata.doc_id = metadata.doc_id.filter(|id| !id.is_empty());
            metadata.collection_id = metadata.collection_id.filter(|id| !id.is_empty());
            metadata.creator = metadata.creator.filter(|creator| !creator.is_empty());
            let outcome = self.merge_with_retries(&entry.cid, &data, metadata).await;
            tracing::debug!(cid = %entry.cid, ?outcome, "Re-drove deferred composite");
            if matches!(outcome, Ok(MergeOutcome::Merged)) {
                if let Some(sink) = self.redriven_merge_sink() {
                    sink.forward(RedrivenMerge::new(&entry, data)).await;
                }
            }
        }
        if self.deferred.has_ready() {
            tracing::debug!("Re-drive budget spent; remaining composites drain on the next merge");
        }
    }
}
