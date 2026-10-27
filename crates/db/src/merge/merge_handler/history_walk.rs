use super::*;

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn history_page(
        &self,
        datastore: &NamespaceView,
        headstore: &NamespaceView,
        store: &NamespaceView,
        root: &Cid,
        root_block: &Block,
        root_payload: &defra_core::block::CompositeDeltaPayload,
        metadata: &BlockMetadata<'_>,
        from_collection: bool,
        merged: &CidSet,
        events: &SegQueue<PendingMergeEvent>,
        actions: &SegQueue<PendingPostCommitAction>,
        fields: &SegQueue<PendingFieldBlockFinalization>,
        context: [u8; 32],
        generation: [u8; 16],
        previous: Option<Progress>,
    ) -> Result<MergeOutcome, MergeError> {
        let budget = self.max_merge_depth.clamp(1, PAGE_STEPS);
        let prefix = format!("/merge-history/v1/{root}/data/");
        let stack_key = |index| format!("{prefix}s/{index:020}").into_bytes();
        let node_key = |cid: &Cid| format!("{prefix}n/{cid}").into_bytes();
        let mut progress = match previous {
            Some(progress) => progress,
            None => {
                let roots = registry::live_roots(store).await?;
                if roots.len() >= MAX_ROOTS {
                    return Ok(if registry::reclaim_idle(store, &roots, budget).await? {
                        MergeOutcome::Yielded
                    } else {
                        MergeOutcome::retryable_skip("history admission quota reached")
                    });
                }
                if let Some(sender) = metadata.sender_peer {
                    if roots
                        .iter()
                        .filter(|(_, peer)| peer.as_deref() == Some(sender))
                        .count()
                        >= MAX_ROOTS_PER_PEER
                    {
                        return Ok(MergeOutcome::retryable_skip(
                            "history admission quota reached for sender",
                        ));
                    }
                }
                if let Some(doc) = metadata
                    .doc_id
                    .filter(|doc| !from_collection && !doc.is_empty())
                {
                    match self
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
                        CompositeMergePreparation::Ready(_) => {}
                        CompositeMergePreparation::Complete(outcome) => return Ok(outcome),
                        CompositeMergePreparation::Deferred { outcome, awaiting } => {
                            self.index_deferred(root, doc, metadata, awaiting);
                            return Ok(outcome);
                        }
                    }
                }
                write(store, &registry::admission_key(root), &metadata.sender_peer).await?;
                write(
                    store,
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
                    generation,
                    last_active_unix: now_unix(),
                    sender_peer: metadata.sender_peer.map(str::to_owned),
                    ..Progress::default()
                }
            }
        };
        if progress.context != context {
            progress.outcome = Some(Completion::Restart);
            progress.context = context;
        }
        if let Some(completion) = progress.outcome.clone() {
            if let Completion::Rejected(reason) = completion {
                return Err(MergeError::HistoryAborted(Box::new(HistoryRejection {
                    root: *root,
                    context,
                    generation,
                    reason,
                })));
            }
            let more = cleanup(store, root, budget).await?;
            if more {
                write(store, &state_key(root), &progress).await?;
            }
            if !more && matches!(completion, Completion::Merged) {
                merged.insert(*root, ());
            }
            return Ok(if more {
                MergeOutcome::Yielded
            } else {
                completion.into_outcome()
            });
        }

        let mut root_checked = false;
        let mut block_reads = super::super::block_reads::BlockReads::default();
        let mut waiting = None;
        let mut slice = workers::Budget::new(16);
        for _ in 0..budget {
            slice.spend(1).await;
            let mut frame: Frame = read(store, &stack_key(progress.top))
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
                    crate::docid::map::get_doc_ids_for_block(store, &cid.to_string()).await?;
                progress.doc = if heads.is_empty() {
                    Some(crate::block::builder::derive_doc_id(&cid))
                } else if owners.len() == 1 {
                    owners.into_iter().next()
                } else {
                    None
                };
            }
            if let Some(doc) = progress.doc.as_deref() {
                if !root_checked {
                    match self
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
                        CompositeMergePreparation::Ready(_) => {}
                        CompositeMergePreparation::Complete(outcome) => {
                            if let MergeOutcome::Rejected { reason } = outcome {
                                return Err(MergeError::RetryRejected(reason));
                            }
                            progress.outcome = Some(match outcome {
                                MergeOutcome::Skipped {
                                    reason,
                                    terminal: false,
                                } => Completion::Deferred(reason),
                                outcome => {
                                    Completion::from_outcome(outcome).unwrap_or(Completion::Restart)
                                }
                            });
                            break;
                        }
                        CompositeMergePreparation::Deferred { outcome, awaiting } => {
                            self.index_deferred(root, doc, metadata, awaiting);
                            progress.outcome = Some(Completion::Deferred(format!("{outcome:?}")));
                            break;
                        }
                    }
                    root_checked = true;
                }
                if Self::composite_was_applied(store, headstore, &cid, payload, doc).await? {
                    store
                        .set(&node_key(&cid), &[1])
                        .await
                        .map_err(storage_error)?;
                    store
                        .delete(&stack_key(progress.top))
                        .await
                        .map_err(storage_error)?;
                    if progress.top == 0 {
                        progress.outcome = Some(Completion::Merged);
                        break;
                    }
                    progress.top -= 1;
                    continue;
                }
            }
            if let Some(parent) = heads.get(frame.next) {
                match store.get(&node_key(parent)).await.map_err(storage_error)? {
                    Some(value) if value.as_ref() == [1] => {
                        frame.next += 1;
                        write(store, &stack_key(progress.top), &frame).await?;
                        continue;
                    }
                    Some(_) => {
                        return Err(MergeError::UnsupportedDelta(
                            "cyclic composite history".into(),
                        ))
                    }
                    None => {}
                }
                frame.next += 1;
                write(store, &stack_key(progress.top), &frame).await?;
                progress.nodes = progress
                    .nodes
                    .checked_add(1)
                    .ok_or_else(|| storage_error("history node count overflow"))?;
                progress.top = progress
                    .top
                    .checked_add(1)
                    .ok_or_else(|| storage_error("history stack overflow"))?;
                write(
                    store,
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
                    self.process_composite_delta_in_txn_body(
                        datastore,
                        headstore,
                        store,
                        &cid,
                        &block,
                        payload,
                        metadata,
                        from_collection,
                        cid == *root,
                        merged,
                        events,
                        actions,
                        fields,
                        doc,
                        collection.map(|collection| *collection),
                        CompositeMergeMode::History,
                        &mut block_reads,
                    )
                    .await?
                }
                CompositeMergePreparation::Complete(outcome) => outcome,
                CompositeMergePreparation::Deferred { outcome, awaiting } => {
                    self.index_deferred(root, doc, metadata, awaiting);
                    outcome
                }
            };
            if let MergeOutcome::Rejected { reason } = outcome {
                return Err(MergeError::RetryRejected(reason));
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
        progress.last_active_unix = now_unix();
        write(store, &state_key(root), &progress).await?;
        Ok(waiting.unwrap_or(MergeOutcome::Yielded))
    }
}
