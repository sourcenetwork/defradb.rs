//! The keys a merge decrypts with, fetched before its transaction: one KMS
//! request for every encrypted field a block brings, per authorization
//! context, so no merge transaction calls the KMS and a composite with F
//! encrypted fields costs one request, not F.
//!
//! A merge's blocks are local before it is admitted (the DAG fetch brought
//! them), so the CIDs are read from the blockstore: an encrypted field
//! block's own, and each field a composite links. A key that is not answered
//! here is asked for the way it was before, when the merge reaches it.

use std::sync::Arc;

use cid::Cid;
use defra_core::block::{Block, CrdtDelta};
use defra_core::merge::BlockMetadata;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashSet;
use storage::corekv::Store;
use zeroize::Zeroizing;

use super::{DbMergeHandler, RetryDecryptionKeys};

/// One fetched key's answer: the key, or the KMS's reason it has none.
pub(crate) type MergeKeyAnswer = Result<Zeroizing<[u8; 32]>, kms::Error>;

/// The answers fetched for one merge, by encryption CID.
pub(crate) type PreparedMergeKeys = Arc<HopscotchMap<Cid, MergeKeyAnswer, RandomState>>;

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// The encryption CIDs merging `block` decrypts with, each once. A link
    /// not local yet adds nothing.
    pub(crate) async fn merge_key_cids(&self, block: &Block) -> Vec<Cid> {
        let mut cids = Vec::new();
        let mut seen = RapidHashSet::default();
        if let (Some(cid), Some(_)) = (block.encryption, block.delta.field()) {
            if seen.insert(cid) {
                cids.push(cid);
            }
        }
        if let (CrdtDelta::Composite(_), Some(links)) = (&block.delta, &block.links) {
            for link in links {
                let Ok(Some(data)) = self.blockstore.get(&link.link).await else {
                    continue;
                };
                if let Some(cid) = Block::from_dag_cbor(&data)
                    .ok()
                    .and_then(|field| field.encryption)
                {
                    if seen.insert(cid) {
                        cids.push(cid);
                    }
                }
            }
        }
        cids
    }

    /// One KMS request for `cids` under `context`, or nothing without a KMS
    /// or a key to ask for. Each answer, by CID.
    pub(crate) async fn fetch_merge_keys(
        &self,
        context: &kms::RequestContext,
        cids: &[Cid],
    ) -> Vec<(Cid, MergeKeyAnswer)> {
        let Some(kms) = self.kms() else {
            return Vec::new();
        };
        if cids.is_empty() {
            return Vec::new();
        }
        match kms.get_keys_each(context, cids).await {
            Ok(answers) => cids
                .iter()
                .copied()
                .zip(answers.into_iter().map(|answer| answer.map(Zeroizing::new)))
                .collect(),
            Err(error) => cids.iter().map(|cid| (*cid, Err(error.clone()))).collect(),
        }
    }

    /// This handler, decrypting with the keys merging `block` needs,
    /// fetched now: a key, or the reason the KMS gave, which the merge meets
    /// where it would have met it asking. A key not fetched here is asked
    /// for when the merge reaches it.
    pub(crate) async fn with_merge_keys(
        &self,
        block: &Block,
        metadata: Option<&BlockMetadata<'_>>,
    ) -> Self {
        let mut handler = self.clone();
        if self.kms().is_none() {
            return handler;
        }
        let cids = self.merge_key_cids(block).await;
        let answers = self
            .fetch_merge_keys(&Self::kms_request_context(metadata), &cids)
            .await;
        if answers.is_empty() {
            return handler;
        }
        let keys: PreparedMergeKeys = Arc::new(HopscotchMap::with_hasher(RandomState::default()));
        keys.extend(answers);
        handler.prepared_keys = Some(keys);
        handler
    }

    /// Fills `keys`, which the attempts of one retried merge share, with the
    /// keys `cids` names that the KMS released under `context`, before the
    /// first attempt. A key it did not release is left out: an attempt asks
    /// for it, and the retry loop reports why it cannot have it, as before.
    pub(crate) async fn prefill_retry_keys(
        &self,
        context: &kms::RequestContext,
        cids: &[Cid],
        keys: &RetryDecryptionKeys,
    ) {
        for (cid, answer) in self.fetch_merge_keys(context, cids).await {
            match answer {
                Ok(key) => {
                    keys.insert(cid, key);
                }
                Err(error) => {
                    tracing::debug!(
                        encryption_cid = %cid,
                        %error,
                        "merge key not released before the merge; the merge asks again"
                    );
                }
            }
        }
    }

    /// This handler, decrypting with this node's own keys only: those of
    /// every encrypted block in the history below `tips`, read from the
    /// blockstore and fetched now. A key held elsewhere is unavailable, and
    /// no peer is asked.
    pub(crate) async fn with_local_history_keys(&self, tips: &[Cid]) -> Self {
        let mut handler = self.clone();
        handler.local_keys_only = true;
        let Some(kms) = self.kms() else {
            return handler;
        };
        let mut cids = Vec::new();
        let mut seen = RapidHashSet::default();
        let mut walk = tips.to_vec();
        while let Some(cid) = walk.pop() {
            if !seen.insert(cid) {
                continue;
            }
            let Ok(Some(data)) = self.blockstore.get(&cid).await else {
                continue;
            };
            let Ok(block) = Block::from_dag_cbor(&data) else {
                continue;
            };
            cids.extend(block.encryption);
            walk.extend(block.heads.into_iter().flatten());
        }
        if cids.is_empty() {
            return handler;
        }
        let context = Self::kms_request_context(None);
        let answers: Vec<(Cid, MergeKeyAnswer)> =
            match kms.get_local_keys_each(&context, &cids).await {
                Ok(answers) => cids
                    .iter()
                    .copied()
                    .zip(answers.into_iter().map(|answer| answer.map(Zeroizing::new)))
                    .collect(),
                Err(error) => cids.iter().map(|cid| (*cid, Err(error.clone()))).collect(),
            };
        let keys: PreparedMergeKeys = Arc::new(HopscotchMap::with_hasher(RandomState::default()));
        keys.extend(answers);
        handler.prepared_keys = Some(keys);
        handler
    }

    /// The answer fetched for `cid` before this merge's transaction, if any.
    pub(crate) fn prepared_key(&self, cid: &Cid) -> Option<MergeKeyAnswer> {
        self.prepared_keys.as_ref()?.get(cid)
    }
}
