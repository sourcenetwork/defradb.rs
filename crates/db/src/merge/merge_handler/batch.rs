use super::hook::CompositePostCommitAction;
use super::*;
use storage::corekv::OnCommit;

use defra_core::merge::MergeBlock;

/// Event collected during batch processing, emitted after commit.
pub(crate) struct PendingMergeEvent {
    pub message: Message,
}

/// Async side effect collected during batch processing, executed after commit.
pub(crate) struct PendingPostCommitAction {
    pub action: Box<dyn CompositePostCommitAction>,
}

/// Field blocks that should be marked merged in the blockstore once the
/// surrounding batch transaction commits. The blockstore's merged-set is the
/// single source of CRDT idempotency; see #847.
pub(crate) struct PendingFieldBlockFinalization {
    pub cids: Vec<Cid>,
}

struct BatchMergeRetryAttempt<S: Store, B: blockstore::Blockstore> {
    handler: DbMergeHandler<S, B>,
    blocks: Vec<MergeBlock>,
    retry_deks: Vec<RetryDecryptionKeys>,
    prefetch_requested: Vec<Arc<std::sync::atomic::AtomicBool>>,
}

impl<S: Store + 'static, B: blockstore::Blockstore + 'static> storage::corekv::RetryAttemptFn
    for BatchMergeRetryAttempt<S, B>
{
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            match self.run_attempt(retry).await {
                Ok(outcomes) => Ok(Box::new(outcomes) as storage::corekv::RetryAttemptValue),
                Err(error) => Err(storage::corekv::RetryAttemptError::Abort(Box::new(error))),
            }
        })
    }
}

impl<S: Store + 'static, B: blockstore::Blockstore + 'static> BatchMergeRetryAttempt<S, B> {
    async fn run_attempt(
        &mut self,
        retry: &mut dyn storage::corekv::RetryTxn,
    ) -> Result<Vec<Result<MergeOutcome, MergeError>>, MergeError> {
        for requested in &self.prefetch_requested {
            requested.store(false, std::sync::atomic::Ordering::Release);
        }

        let io = retry
            .begin_attempt_io()
            .map_err(|error| MergeError::Database(crate::error::Error::Storage(error)))?;
        let basic_txn = datastore::BasicTxn::from_retry_attempt_io(io, 0);
        let txn = self.handler.db.new_db_txn(basic_txn);
        for block in &self.blocks {
            if let Some(collection) = self
                .handler
                .block_collection(&block.collection_id, None)
                .await?
            {
                txn.validate_collection_schema_for_write(&collection)
                    .await
                    .map_err(MergeError::Database)?;
            }
        }
        let batch_merged = cid_set();
        let batch_merged_collections = cid_set();
        let pending_events: SegQueue<PendingMergeEvent> = SegQueue::new();
        let pending_post_commit_actions: SegQueue<PendingPostCommitAction> = SegQueue::new();
        let pending_field_block_finalizations: SegQueue<PendingFieldBlockFinalization> =
            SegQueue::new();
        let mut outcomes = Vec::with_capacity(self.blocks.len());
        let mut batch_error = None;

        {
            let datastore = txn.datastore()?;
            let headstore = txn.headstore()?;
            let systemstore = txn.systemstore()?;
            for (block_index, block) in self.blocks.iter().enumerate() {
                let block_handler = self.handler.with_retry_decryption_context(
                    Arc::clone(&self.retry_deks[block_index]),
                    Arc::clone(&self.prefetch_requested[block_index]),
                );
                let metadata = batch_block_metadata(block);
                let result = block_handler
                    .process_block_in_txn(
                        &datastore,
                        &headstore,
                        &systemstore,
                        &block.cid,
                        &block.block_data,
                        &metadata,
                        &batch_merged,
                        &batch_merged_collections,
                        &pending_events,
                        &pending_post_commit_actions,
                        &pending_field_block_finalizations,
                    )
                    .await;
                match result {
                    Ok(MergeOutcome::Rejected { reason }) => {
                        batch_error = Some(MergeError::UniqueConstraintViolation(reason));
                        break;
                    }
                    Ok(outcome) => outcomes.push(Ok(outcome)),
                    Err(MergeError::NeedsDecryptionKey(encryption_cid)) => {
                        batch_error = Some(MergeError::NeedsBatchDecryptionKey {
                            block_index,
                            encryption_cid,
                        });
                        break;
                    }
                    Err(error) => {
                        batch_error = Some(error);
                        break;
                    }
                }
            }
        }

        if let Some(error) = batch_error {
            let _ = txn.discard();
            return Err(error);
        }

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
                    tracing::warn!(error = %error, "Post-commit batch merge action failed");
                }
            }
            handler
                .best_effort_finalize_linked_field_blocks(&field_cids)
                .await;
        };

        if events.is_empty() {
            txn.on_commit(OnCommit::Async(Box::new(move || Box::pin(after_commit))))?;
        } else {
            txn.on_commit(OnCommit::OrderedAsync(Box::new(move || {
                Box::pin(async move {
                    after_commit.await;
                    Ok(())
                })
            })))?;
            if let Some(bus) = self.handler.db.event_bus().cloned() {
                txn.on_commit(OnCommit::Ordered(Box::new(move || {
                    bus.publish_batch(events)
                })))?;
            }
        }

        let (io, callbacks) = txn
            .into_retry_attempt()
            .map_err(crate::error::Error::Storage)?;
        retry
            .finish_attempt_io(io, callbacks)
            .map_err(|error| MergeError::Database(crate::error::Error::Storage(error)))?;
        Ok(outcomes)
    }
}

fn batch_block_metadata(block: &MergeBlock) -> BlockMetadata<'_> {
    BlockMetadata::normal(
        &block.doc_id,
        &block.collection_id,
        &block.creator,
        block.sender_peer.as_deref(),
        block.is_explicit_replicator,
    )
    .with_explicit_replay_authorization(block.explicit_replay_authorization.clone())
}

impl<S: Store + 'static, B: blockstore::Blockstore + 'static> DbMergeHandler<S, B> {
    /// Process blocks individually, each with its own transaction.
    pub(crate) async fn merge_blocks_individually(
        &self,
        blocks: &[MergeBlock],
    ) -> Vec<Result<MergeOutcome, MergeError>> {
        let mut results = Vec::with_capacity(blocks.len());
        for block in blocks {
            if let Err(error) = self
                .validate_explicit_replay_authorization(
                    block.explicit_replay_authorization.as_ref(),
                    block,
                )
                .await
            {
                results.push(Err(error));
                continue;
            }

            let metadata = BlockMetadata::normal(
                &block.doc_id,
                &block.collection_id,
                &block.creator,
                block.sender_peer.as_deref(),
                block.is_explicit_replicator,
            )
            .with_explicit_replay_authorization(block.explicit_replay_authorization.clone());

            // Conflict retry lives inside `handle_block` (Go's MaxTxnRetries
            // parity), so every caller — including the parallel replication
            // workers — gets it, not just this batch fallback.
            results.push(
                self.handle_block(&block.cid, &block.block_data, metadata)
                    .await,
            );
        }
        results
    }

    /// Attempt batch merge with binary-split retry on failure.
    ///
    /// Tries the whole batch first. On failure, splits into two halves and
    /// recurses on each half. A single root also uses the shared transaction:
    /// its history may contain thousands of revisions. Only a failed singleton
    /// falls back to individual processing. This isolates bad blocks with ~log2(N) batch attempts
    /// instead of falling back to N individual transactions.
    #[allow(clippy::type_complexity)]
    pub(crate) fn try_batch_merge_with_split<'a>(
        &'a self,
        blocks: &'a [MergeBlock],
    ) -> defra_core::thread_bounds::MaybeBoxFuture<'a, Vec<Result<MergeOutcome, MergeError>>> {
        Box::pin(async move {
            if blocks.is_empty() {
                return Vec::new();
            }

            match self.try_batch_merge(blocks).await {
                Ok(results) => results,
                Err(e) => {
                    if blocks.len() == 1 {
                        return self.merge_blocks_individually(blocks).await;
                    }
                    tracing::debug!(
                        error = %e,
                        batch_size = blocks.len(),
                        "Batch merge failed, splitting in half"
                    );
                    let mid = blocks.len() / 2;
                    let (left, right) = blocks.split_at(mid);
                    let mut results = self.try_batch_merge_with_split(left).await;
                    results.extend(self.try_batch_merge_with_split(right).await);
                    results
                }
            }
        })
    }

    /// Attempt to merge all blocks within a single shared transaction.
    ///
    /// If any block fails, the entire transaction is rolled back and the caller
    /// should fall back to per-block processing.
    pub async fn try_batch_merge(
        &self,
        blocks: &[MergeBlock],
    ) -> Result<Vec<Result<MergeOutcome, MergeError>>, MergeError> {
        if blocks.is_empty() {
            return Ok(Vec::new());
        }

        // Authorization is immutable request input, so validate it before the
        // engine starts replaying transaction attempts.
        for block in blocks {
            self.validate_explicit_replay_authorization(
                block.explicit_replay_authorization.as_ref(),
                block,
            )
            .await?;
        }

        // Never share a CID-only memo across block authorization contexts.
        // Each block owns a small key map, even when two blocks reference the
        // same encryption block under different explicit replay principals.
        let retry_deks: Vec<RetryDecryptionKeys> =
            blocks.iter().map(|_| retry_decryption_keys()).collect();
        let prefetch_requested: Vec<_> = blocks
            .iter()
            .map(|_| Arc::new(AtomicBool::new(false)))
            .collect();
        let kms_contexts: Vec<_> = blocks
            .iter()
            .map(|block| Self::kms_request_context(Some(&batch_block_metadata(block))))
            .collect();
        self.prefill_batch_keys(blocks, &kms_contexts, &retry_deks)
            .await;

        let result = loop {
            let attempt = BatchMergeRetryAttempt {
                handler: self.clone(),
                blocks: blocks.to_vec(),
                retry_deks: retry_deks.clone(),
                prefetch_requested: prefetch_requested.clone(),
            };
            match self
                .db
                .store()
                .transact_retry(self.db.options().retry_options(), Box::new(attempt))
                .await
            {
                Ok(value) => {
                    break value
                        .downcast::<Vec<Result<MergeOutcome, MergeError>>>()
                        .map(|outcomes| *outcomes)
                        .map_err(|_| {
                            MergeError::MergeFailed("invalid batch merge retry result".into())
                        });
                }
                Err(error) => match error.downcast_abort::<MergeError>() {
                    Ok(MergeError::NeedsBatchDecryptionKey {
                        block_index,
                        encryption_cid,
                    }) => {
                        let Some(kms) = self.kms() else {
                            break Err(MergeError::Kms(kms::Error::KeyUnavailable));
                        };
                        let (Some(context), Some(block_keys)) =
                            (kms_contexts.get(block_index), retry_deks.get(block_index))
                        else {
                            break Err(MergeError::MergeFailed(
                                "batch merge requested a DEK for an invalid block index".into(),
                            ));
                        };
                        if block_keys.contains_key(&encryption_cid) {
                            break Err(repeated_key_request(&encryption_cid));
                        }
                        match kms
                            .get_keys(context, std::slice::from_ref(&encryption_cid))
                            .await
                        {
                            Ok(results) => match results.wait_all().await {
                                Ok(mut keys) => match keys.remove(&encryption_cid) {
                                    Some(key) => {
                                        block_keys.insert(encryption_cid, Zeroizing::new(key));
                                    }
                                    None => break Err(MergeError::Kms(kms::Error::KeyUnavailable)),
                                },
                                Err(error) => break Err(MergeError::Kms(error)),
                            },
                            Err(error) => break Err(MergeError::Kms(error)),
                        }
                    }
                    Ok(error) => break Err(error),
                    Err(error) => {
                        break Err(MergeError::Database(crate::error::Error::Storage(
                            error.into_storage(),
                        )));
                    }
                },
            }
        };

        // Start ownership-gate prefetches only after the engine has stopped
        // invoking attempts. Each uses the corresponding block's auth context.
        for (index, block) in blocks.iter().enumerate() {
            if prefetch_requested[index].load(std::sync::atomic::Ordering::Acquire) {
                if let Ok(decoded) = Block::from_dag_cbor(&block.block_data) {
                    if let Some(encryption_cid) = decoded.encryption {
                        self.spawn_dek_prefetch(encryption_cid, &batch_block_metadata(block));
                    }
                }
            }
        }

        result
    }

    /// Fills each block's key map before the first attempt: one KMS request
    /// per authorization context for every key its blocks decrypt with, and
    /// each block given only the keys asked under its own context.
    async fn prefill_batch_keys(
        &self,
        blocks: &[MergeBlock],
        contexts: &[kms::RequestContext],
        retry_deks: &[RetryDecryptionKeys],
    ) {
        if self.kms().is_none() {
            return;
        }
        let mut needed = Vec::with_capacity(blocks.len());
        for block in blocks {
            needed.push(match Block::from_dag_cbor(&block.block_data) {
                Ok(decoded) => self.merge_key_cids(&decoded).await,
                Err(_) => Vec::new(),
            });
        }
        let mut grouped: Vec<(&kms::RequestContext, Vec<usize>)> = Vec::new();
        for (index, context) in contexts.iter().enumerate() {
            match grouped.iter_mut().find(|(seen, _)| *seen == context) {
                Some((_, members)) => members.push(index),
                None => grouped.push((context, vec![index])),
            }
        }
        for (context, members) in grouped {
            let mut cids: Vec<Cid> = members
                .iter()
                .flat_map(|index| needed[*index].iter().copied())
                .collect();
            cids.sort_unstable();
            cids.dedup();
            let answers = self.fetch_merge_keys(context, &cids).await;
            for index in members {
                for (cid, answer) in &answers {
                    if let (Ok(key), true) = (answer, needed[index].contains(cid)) {
                        retry_deks[index].insert(*cid, key.clone());
                    }
                }
            }
        }
    }

    /// Decode and apply a block within the caller-owned attempt.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn process_block_in_txn(
        &self,
        datastore: &NamespaceView,
        headstore: &NamespaceView,
        systemstore: &NamespaceView,
        cid: &Cid,
        block_data: &[u8],
        metadata: &BlockMetadata<'_>,
        batch_merged: &CidSet,
        batch_merged_collections: &CidSet,
        pending_events: &SegQueue<PendingMergeEvent>,
        pending_post_commit_actions: &SegQueue<PendingPostCommitAction>,
        pending_field_block_finalizations: &SegQueue<PendingFieldBlockFinalization>,
    ) -> Result<MergeOutcome, MergeError> {
        // Decode the block from DAG-CBOR
        let block =
            Block::from_dag_cbor(block_data).map_err(|e| MergeError::BlockDecode(e.to_string()))?;

        // Verify block signature (batch path). Clone metadata so we can
        // set verified_creator from the cryptographic verification result.
        let mut metadata = metadata.clone();
        if !metadata.is_recovery {
            let verified = self.verify_block_signature(cid, &block, block_data).await?;
            metadata.verified_creator = verified;
        }

        // Same ownership-before-decryption gate as the non-batch dispatch in
        // `mod.rs::handle_block`: a standalone encrypted field block can only
        // merge once the ownership index names it a single owner, so check
        // that before paying for a (possibly cross-network) KMS fetch whose
        // result would just be discarded by the dispatch below.
        if block.encryption.is_some()
            && matches!(block.delta, CrdtDelta::Lww(_) | CrdtDelta::Counter(_))
            && self
                .resolve_field_block_identity(systemstore, cid)
                .await?
                .is_none()
        {
            // Still REQUEST the DEK (detached) — see the twin gate in
            // `mod.rs::handle_block` for the Go-parity rationale.
            if let Some(enc_cid) = block.encryption {
                if !self.request_retry_dek_prefetch() {
                    self.spawn_dek_prefetch(enc_cid, &metadata);
                }
            }
            return Ok(MergeOutcome::terminal_skip(
                "field block has no unambiguous owner; merged via its composite",
            ));
        }

        // Decrypt delta data if the block has encryption
        let decrypted_block;
        let effective_block = if block.encryption.is_some() {
            match &block.delta {
                CrdtDelta::Lww(payload) => {
                    match self
                        .decrypt_block_data(
                            &payload.data,
                            block.encryption.as_ref(),
                            Some(&metadata),
                        )
                        .await
                    {
                        Ok(decrypted_data) => {
                            let mut new_payload = payload.clone();
                            new_payload.data = decrypted_data;
                            decrypted_block = Block {
                                delta: CrdtDelta::Lww(new_payload),
                                heads: block.heads.clone(),
                                links: block.links.clone(),
                                encryption: block.encryption,
                                signature: block.signature,
                            };
                            &decrypted_block
                        }
                        Err(MergeError::Kms(kms::Error::AccessDenied { .. })) => {
                            return Ok(MergeOutcome::terminal_skip(
                                "encryption key unavailable for standalone field block",
                            ));
                        }
                        Err(error @ MergeError::NeedsDecryptionKey(_)) => return Err(error),
                        Err(error @ MergeError::Kms(_)) => return Err(error),
                        Err(_) => &block,
                    }
                }
                CrdtDelta::Counter(payload) => {
                    match self
                        .decrypt_block_data(
                            &payload.data,
                            block.encryption.as_ref(),
                            Some(&metadata),
                        )
                        .await
                    {
                        Ok(decrypted_data) => {
                            let mut new_payload = payload.clone();
                            new_payload.data = decrypted_data;
                            decrypted_block = Block {
                                delta: CrdtDelta::Counter(new_payload),
                                heads: block.heads.clone(),
                                links: block.links.clone(),
                                encryption: block.encryption,
                                signature: block.signature,
                            };
                            &decrypted_block
                        }
                        Err(MergeError::Kms(kms::Error::AccessDenied { .. })) => {
                            return Ok(MergeOutcome::terminal_skip(
                                "encryption key unavailable for standalone field block",
                            ));
                        }
                        Err(error @ MergeError::NeedsDecryptionKey(_)) => return Err(error),
                        Err(error @ MergeError::Kms(_)) => return Err(error),
                        Err(_) => &block,
                    }
                }
                _ => &block,
            }
        } else {
            &block
        };

        // Dispatch based on delta type
        match &effective_block.delta {
            CrdtDelta::Composite(payload) => {
                self.process_composite_delta_in_txn(
                    datastore,
                    headstore,
                    systemstore,
                    cid,
                    &block,
                    payload,
                    &metadata,
                    false,
                    batch_merged,
                    batch_merged_collections,
                    pending_events,
                    pending_post_commit_actions,
                    pending_field_block_finalizations,
                    0,
                )
                .await
            }
            CrdtDelta::Collection(payload) => {
                self.process_collection_delta_in_txn(
                    datastore,
                    headstore,
                    systemstore,
                    cid,
                    &block,
                    payload,
                    &metadata,
                    batch_merged,
                    batch_merged_collections,
                    pending_events,
                    pending_post_commit_actions,
                    pending_field_block_finalizations,
                    0,
                )
                .await
            }
            // A Text or Json block edits its document's stored value and
            // heads, which only its composite writes.
            CrdtDelta::Text(_) => Ok(MergeOutcome::terminal_skip(
                "text block merged via its composite",
            )),
            CrdtDelta::Json(_) => Ok(MergeOutcome::terminal_skip(
                "json block merged via its composite",
            )),
            CrdtDelta::Lww(payload) => {
                let Some((doc_id_str, doc_short_id)) =
                    self.resolve_field_block_identity(systemstore, cid).await?
                else {
                    return Ok(MergeOutcome::terminal_skip(
                        "field block has no unambiguous owner; merged via its composite",
                    ));
                };
                let mut ds = datastore.clone();
                let result = self
                    .process_lww_delta_in_txn(
                        &mut ds,
                        headstore,
                        cid,
                        payload,
                        metadata.collection_id,
                        &doc_id_str,
                        doc_short_id,
                        &mut super::block_reads::BlockReads::default(),
                    )
                    .await;
                match result {
                    Ok(r) if r.applied => Ok(MergeOutcome::Merged),
                    Ok(_) => Ok(MergeOutcome::terminal_skip("rejected by CRDT")),
                    Err(e) => Err(e),
                }
            }
            CrdtDelta::Counter(payload) => {
                let Some((doc_id_str, doc_short_id)) =
                    self.resolve_field_block_identity(systemstore, cid).await?
                else {
                    return Ok(MergeOutcome::terminal_skip(
                        "field block has no unambiguous owner; merged via its composite",
                    ));
                };
                let mut ds = datastore.clone();
                let result = self
                    .process_counter_delta_in_txn(
                        &mut ds,
                        headstore,
                        cid,
                        payload,
                        metadata.collection_id,
                        &doc_id_str,
                        doc_short_id,
                    )
                    .await;
                match result {
                    Ok(r) if r.applied => {
                        pending_field_block_finalizations
                            .push(PendingFieldBlockFinalization { cids: vec![*cid] });
                        Ok(MergeOutcome::Merged)
                    }
                    Ok(_) => {
                        pending_field_block_finalizations
                            .push(PendingFieldBlockFinalization { cids: vec![*cid] });
                        Ok(MergeOutcome::terminal_skip("rejected by CRDT"))
                    }
                    Err(e) => Err(e),
                }
            }
            CrdtDelta::FieldDefinition(_) => Ok(MergeOutcome::terminal_skip(
                "field definition processed with collection",
            )),
            CrdtDelta::CollectionDefinition(payload) => {
                // CollectionDefinition uses its own txn (rare, not worth batching)
                self.process_collection_definition_delta(cid, &block, payload, &metadata)
                    .await
            }
            CrdtDelta::Immutable(_) => Ok(MergeOutcome::terminal_skip(
                "immutable block merged via its composite",
            )),
            CrdtDelta::CollectionSet(_) => Ok(MergeOutcome::terminal_skip("collection set delta")),
            // Only the variant discriminant is reported — `CrdtDelta` carries
            // field-value bytes from user documents and must not be formatted
            // into error strings that may end up in logs.
            other => Err(MergeError::UnsupportedDelta(format!(
                "unhandled CrdtDelta variant in batch dispatch: {:?}",
                std::mem::discriminant(other)
            ))),
        }
    }
}
