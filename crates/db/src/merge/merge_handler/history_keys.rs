use super::*;

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// A bounded preview for KMS batching only. The writing attempt re-reads
    /// every cursor and publication; this snapshot never authorizes a write.
    pub(in crate::merge::merge_handler) async fn history_key_cids(
        &self,
        block: &Block,
        root: &Cid,
    ) -> Result<Vec<Cid>, MergeError> {
        if self.kms().is_none() {
            return Ok(Vec::new());
        }
        let txn = self.db.new_txn(true).await?;
        let store = txn.systemstore()?;
        let heads = txn.headstore()?;
        let mut cids = Vec::new();
        let mut remaining = self.max_merge_depth.clamp(1, PAGE_STEPS);
        match &block.delta {
            CrdtDelta::Composite(_) => {
                self.preview_history_keys(&store, &heads, root, &mut cids, &mut remaining)
                    .await?;
            }
            CrdtDelta::Collection(_) => {
                for link in block.links.as_deref().unwrap_or_default() {
                    if remaining == 0 {
                        break;
                    }
                    self.preview_history_keys(
                        &store,
                        &heads,
                        &link.link,
                        &mut cids,
                        &mut remaining,
                    )
                    .await?;
                }
            }
            _ => {}
        }
        cids.sort_unstable();
        cids.dedup();
        Ok(cids)
    }

    async fn preview_history_keys(
        &self,
        store: &NamespaceView,
        heads: &NamespaceView,
        root: &Cid,
        cids: &mut Vec<Cid>,
        remaining: &mut usize,
    ) -> Result<(), MergeError> {
        let progress = read::<Progress>(store, &state_key(root)).await?;
        if progress
            .as_ref()
            .is_some_and(|progress| progress.outcome.is_some())
        {
            return Ok(());
        }
        let prefix = format!("/merge-history/v1/{root}/data/");
        let stack_key = |index| format!("{prefix}s/{index:020}").into_bytes();
        let node_key = |cid: &Cid| format!("{prefix}n/{cid}").into_bytes();
        let mut top = progress.as_ref().map_or(0, |progress| progress.top);
        let doc = progress
            .as_ref()
            .and_then(|progress| progress.doc.as_deref());
        let mut frames = RapidHashMap::default();
        let mut nodes = RapidHashMap::default();
        if progress.is_none() {
            frames.insert(
                0,
                Frame {
                    cid: root.to_string(),
                    next: 0,
                },
            );
            nodes.insert(*root, 0);
        }
        let mut slice = workers::Budget::new(16);
        while *remaining > 0 {
            slice.spend(1).await;
            *remaining -= 1;
            let mut frame = match frames.remove(&top) {
                Some(frame) => frame,
                None => match read::<Frame>(store, &stack_key(top)).await? {
                    Some(frame) => frame,
                    None => break,
                },
            };
            let cid = frame.cid.parse().map_err(storage_error)?;
            let Ok(Some(bytes)) = self.blockstore.get(&cid).await else {
                break;
            };
            let Ok(block) = Block::from_dag_cbor(&bytes) else {
                break;
            };
            let CrdtDelta::Composite(payload) = &block.delta else {
                break;
            };
            let applied = if let Some(doc) = doc {
                Self::composite_was_applied(store, heads, &cid, payload, doc).await?
            } else {
                false
            };
            if !applied {
                if let Some(parent) = block.heads.as_deref().unwrap_or_default().get(frame.next) {
                    let status = match nodes.get(parent).copied() {
                        Some(status) => Some(status),
                        None => store
                            .get(&node_key(parent))
                            .await
                            .map_err(storage_error)?
                            .and_then(|bytes| bytes.first().copied()),
                    };
                    if status == Some(0) {
                        break;
                    }
                    frame.next += 1;
                    frames.insert(top, frame);
                    if status != Some(1) {
                        top = top
                            .checked_add(1)
                            .ok_or_else(|| storage_error("history stack overflow"))?;
                        frames.insert(
                            top,
                            Frame {
                                cid: parent.to_string(),
                                next: 0,
                            },
                        );
                        nodes.insert(*parent, 0);
                    }
                    continue;
                }
                cids.extend(self.merge_key_cids(&block).await);
            }
            nodes.insert(cid, 1);
            if top == 0 {
                break;
            }
            top -= 1;
        }
        Ok(())
    }
}
