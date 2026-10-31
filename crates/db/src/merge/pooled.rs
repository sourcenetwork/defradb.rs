//! The boundary between the P2P layer and the worker pool for merges: a
//! [`MergeHandler`] that runs each merge on the worker of the document it
//! merges, keyed by the collection's ID, one task per document at a time
//! (`query::pooled::document_key`). The P2P layer receives blocks
//! on the network edge; the merge itself, a storage transaction, never runs
//! there.
//!
//! A batch that spans documents is split by document, each part merged on
//! its document's worker, the parts handed to the pool together
//! (`workers::Pool::submit_batch`: one wake per worker). A merge whose
//! document is not known yet (crash recovery reads it from the block) runs
//! on the least-loaded worker: the storage engine validates every commit
//! wherever it runs. Called on a worker, a merge runs there; without a pool
//! (a test, a one-shot tool) where it is called.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use defra_core::merge::{
    BlockMetadata, ExplicitReplayAuthorization, MergeBlock, MergeErrorDisposition, MergeHandler,
    MergeOutcome, RecoveredBlockMetadata,
};

/// `inner`'s merges, run on the pool's workers.
pub struct PooledMergeHandler<H> {
    inner: Arc<H>,
    pool: Option<workers::Pool>,
    admission: regolith::sync::Semaphore,
}

impl<H> PooledMergeHandler<H> {
    /// Runs `inner`'s merges on `pool`; where they are called without one.
    pub fn new(inner: Arc<H>, pool: Option<workers::Pool>) -> Self {
        Self {
            inner,
            pool,
            admission: regolith::sync::Semaphore::new(64),
        }
    }
}

/// A block's metadata, owned, so it can cross to the merge's worker.
struct OwnedMetadata {
    doc_id: Option<String>,
    collection_id: Option<String>,
    creator: Option<String>,
    sender_peer: Option<String>,
    is_explicit_replicator: bool,
    explicit_replay_authorization: Option<ExplicitReplayAuthorization>,
    verified_creator: Option<String>,
    is_recovery: bool,
    is_schema_block: bool,
    is_restore: bool,
}

impl OwnedMetadata {
    fn of(metadata: &BlockMetadata<'_>) -> Self {
        let owned = |value: Option<&str>| value.map(str::to_owned);
        Self {
            doc_id: owned(metadata.doc_id),
            collection_id: owned(metadata.collection_id),
            creator: owned(metadata.creator),
            sender_peer: owned(metadata.sender_peer),
            is_explicit_replicator: metadata.is_explicit_replicator,
            explicit_replay_authorization: metadata.explicit_replay_authorization.clone(),
            verified_creator: metadata.verified_creator.clone(),
            is_recovery: metadata.is_recovery,
            is_schema_block: metadata.is_schema_block,
            is_restore: metadata.is_restore,
        }
    }

    fn borrowed(&self) -> BlockMetadata<'_> {
        BlockMetadata {
            doc_id: self.doc_id.as_deref(),
            collection_id: self.collection_id.as_deref(),
            creator: self.creator.as_deref(),
            sender_peer: self.sender_peer.as_deref(),
            is_explicit_replicator: self.is_explicit_replicator,
            explicit_replay_authorization: self.explicit_replay_authorization.clone(),
            verified_creator: self.verified_creator.clone(),
            is_recovery: self.is_recovery,
            is_schema_block: self.is_schema_block,
            is_restore: self.is_restore,
        }
    }
}

/// The worker of the document `collection_id` and `doc_id` name, when both
/// are known.
fn document_target(collection_id: Option<&str>, doc_id: Option<&str>) -> workers::Target {
    match (collection_id, doc_id) {
        (Some(collection), Some(doc)) if !collection.is_empty() && !doc.is_empty() => {
            workers::Target::Key(query::pooled::document_key(collection, doc))
        }
        _ => workers::Target::LeastLoaded,
    }
}

/// One document's worker when every block of `blocks` is of that document.
fn batch_target(blocks: &[MergeBlock]) -> workers::Target {
    let Some(first) = blocks.first() else {
        return workers::Target::LeastLoaded;
    };
    let one_document = blocks
        .iter()
        .all(|block| block.collection_id == first.collection_id && block.doc_id == first.doc_id);
    if one_document {
        document_target(Some(&first.collection_id), Some(&first.doc_id))
    } else {
        workers::Target::LeastLoaded
    }
}

impl<H> PooledMergeHandler<H>
where
    H: MergeHandler + 'static,
    H::Error: From<workers::Error> + Send,
{
    /// The pool to hand a merge to: none on a worker, where it runs in place
    /// with nothing copied, or without a pool.
    fn off_the_pool(&self) -> Option<&workers::Pool> {
        self.pool.as_ref().filter(|_| !workers::on_worker())
    }

    /// Runs `merge` on `inner` on the worker `target` names.
    async fn on_pool<T, F, Fut>(
        &self,
        pool: &workers::Pool,
        target: workers::Target,
        merge: F,
    ) -> Result<T, H::Error>
    where
        F: FnOnce(Arc<H>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, H::Error>> + 'static,
        T: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        workers::run_on(Some(pool), target, move || merge(inner)).await?
    }
}

#[async_trait]
impl<H> MergeHandler for PooledMergeHandler<H>
where
    H: MergeHandler + 'static,
    H::Error: From<workers::Error> + Send,
{
    type Error = H::Error;

    fn error_disposition(&self, error: &Self::Error) -> MergeErrorDisposition {
        self.inner.error_disposition(error)
    }

    async fn handle_block(
        &self,
        cid: &Cid,
        block_data: &[u8],
        metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, Self::Error> {
        let Some(pool) = self.off_the_pool() else {
            return self.inner.handle_block(cid, block_data, metadata).await;
        };
        let _admission = self.admission.acquire(1).await;
        let target = document_target(metadata.collection_id, metadata.doc_id);
        let (cid, data, metadata) = (
            *cid,
            Bytes::copy_from_slice(block_data),
            OwnedMetadata::of(&metadata),
        );
        self.on_pool(pool, target, move |inner| async move {
            inner.handle_block(&cid, &data, metadata.borrowed()).await
        })
        .await
    }

    async fn validate_authorization(
        &self,
        authorization: Option<&ExplicitReplayAuthorization>,
        block: &MergeBlock,
    ) -> Result<(), Self::Error> {
        let Some(pool) = self.off_the_pool() else {
            return self
                .inner
                .validate_authorization(authorization, block)
                .await;
        };
        let target = document_target(Some(&block.collection_id), Some(&block.doc_id));
        let (authorization, block) = (authorization.cloned(), block.clone());
        self.on_pool(pool, target, move |inner| async move {
            inner
                .validate_authorization(authorization.as_ref(), &block)
                .await
        })
        .await
    }

    async fn recover_block_metadata(
        &self,
        cid: &Cid,
        block_data: &[u8],
    ) -> Result<Option<RecoveredBlockMetadata>, Self::Error> {
        let Some(pool) = self.off_the_pool() else {
            return self.inner.recover_block_metadata(cid, block_data).await;
        };
        let (cid, data) = (*cid, Bytes::copy_from_slice(block_data));
        self.on_pool(
            pool,
            workers::Target::LeastLoaded,
            move |inner| async move { inner.recover_block_metadata(&cid, &data).await },
        )
        .await
    }

    async fn handle_block_batch(
        &self,
        blocks: &[MergeBlock],
    ) -> Vec<Result<MergeOutcome, Self::Error>> {
        let Some(pool) = self.off_the_pool() else {
            return self.inner.handle_block_batch(blocks).await;
        };
        let _admission = self.admission.acquire(1).await;
        let documents = by_document(blocks);
        if documents.len() < 2 {
            let (count, blocks) = (blocks.len(), blocks.to_vec());
            let inner = Arc::clone(&self.inner);
            let ran = workers::run_on(Some(pool), batch_target(&blocks), move || async move {
                inner.handle_block_batch(&blocks).await
            });
            return outcomes(count, ran.await);
        }
        // Several documents: each one's blocks merge on its worker, the parts
        // handed over together.
        let mut jobs = Vec::with_capacity(documents.len());
        for indices in &documents {
            let part: Vec<MergeBlock> =
                indices.iter().map(|&index| blocks[index].clone()).collect();
            let inner = Arc::clone(&self.inner);
            jobs.push((batch_target(&part), move || async move {
                inner.handle_block_batch(&part).await
            }));
        }
        let mut placed = Vec::with_capacity(blocks.len());
        for (indices, submitted) in documents.iter().zip(pool.submit_batch(jobs).await) {
            let ran = match submitted {
                Ok(completion) => completion.await,
                Err(error) => Err(error),
            };
            placed.extend(indices.iter().copied().zip(outcomes(indices.len(), ran)));
        }
        placed.sort_by_key(|(index, _)| *index);
        placed.into_iter().map(|(_, outcome)| outcome).collect()
    }
}

/// Where each document's blocks sit in `blocks`, in their order.
fn by_document(blocks: &[MergeBlock]) -> Vec<Vec<usize>> {
    let document = |index: &usize| (&blocks[*index].collection_id, &blocks[*index].doc_id);
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    // Stable: a document's blocks keep their order.
    order.sort_by(|a, b| document(a).cmp(&document(b)));
    order
        .chunk_by(|a, b| document(a) == document(b))
        .map(<[usize]>::to_vec)
        .collect()
}

/// What `count` blocks merged to, or when the pool could not run them, a
/// failure for each, which stays retryable.
fn outcomes<E: From<workers::Error>>(
    count: usize,
    ran: Result<Vec<Result<MergeOutcome, E>>, workers::Error>,
) -> Vec<Result<MergeOutcome, E>> {
    ran.unwrap_or_else(|error| (0..count).map(|_| Err(E::from(error.clone()))).collect())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod batch_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn block(collection: &str, doc: &str) -> MergeBlock {
        MergeBlock {
            cid: Cid::default(),
            block_data: Bytes::new(),
            doc_id: doc.into(),
            collection_id: collection.into(),
            creator: String::new(),
            sender_peer: None,
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        }
    }

    #[test]
    fn a_merge_goes_to_its_documents_worker_and_a_mixed_batch_anywhere() {
        let key = workers::Target::Key(query::pooled::document_key("c1", "bae-1"));
        assert_eq!(document_target(Some("c1"), Some("bae-1")), key);
        assert_eq!(
            document_target(None, Some("bae-1")),
            workers::Target::LeastLoaded
        );
        assert_eq!(
            batch_target(&[block("c1", "bae-1"), block("c1", "bae-1")]),
            key
        );
        assert_eq!(
            batch_target(&[block("c1", "bae-1"), block("c1", "bae-2")]),
            workers::Target::LeastLoaded
        );
        assert_eq!(batch_target(&[]), workers::Target::LeastLoaded);
    }

    #[test]
    fn metadata_survives_the_crossing_unchanged() {
        let metadata = BlockMetadata {
            doc_id: Some("bae-1"),
            collection_id: Some("c1"),
            creator: Some("did:key:a"),
            sender_peer: Some("peer"),
            is_explicit_replicator: true,
            explicit_replay_authorization: None,
            verified_creator: Some("did:key:b".into()),
            is_recovery: false,
            is_schema_block: true,
            is_restore: true,
        };
        let owned = OwnedMetadata::of(&metadata);
        let back = owned.borrowed();
        assert_eq!(back.doc_id, metadata.doc_id);
        assert_eq!(back.collection_id, metadata.collection_id);
        assert_eq!(back.creator, metadata.creator);
        assert_eq!(back.sender_peer, metadata.sender_peer);
        assert_eq!(back.verified_creator, metadata.verified_creator);
        assert!(back.is_explicit_replicator && back.is_schema_block && !back.is_recovery);
        assert!(back.is_restore);
    }
}
