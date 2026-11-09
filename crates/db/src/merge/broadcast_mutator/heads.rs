use std::sync::Arc;

use blockstore::Blockstore;
use bytes::Bytes;
use cid::Cid;
use futures::channel::oneshot;
use p2p::sync::SyncCoordinator;
use p2p::transport::P2PTransport;

use crate::block::builder::BlockResult;

/// What a filtered replicator decides a committed head by.
pub(super) enum DecidedBy {
    /// The document's own fields, when it encoded.
    Own(Option<serde_json::Value>),
    /// The parent's fields, for a `@chunk` chunk.
    Parent(Option<Arc<serde_json::Value>>),
    /// A branchable collection's head, which no filter decides.
    Collection,
}

/// One committed head that still needs its sender markers.
pub(super) struct CommittedHead {
    pub(super) cid: Cid,
    pub(super) block: Bytes,
    pub(super) doc_id: String,
    pub(super) collection_id: String,
    pub(super) creator: Option<String>,
    pub(super) decided_by: DecidedBy,
}

impl CommittedHead {
    pub(super) fn of(
        block: &BlockResult,
        collection_id: &str,
        creator: Option<&str>,
        decided_by: DecidedBy,
    ) -> Self {
        Self {
            cid: block.cid,
            block: block.block.clone(),
            doc_id: block.doc_id.clone(),
            collection_id: collection_id.to_string(),
            creator: creator.map(str::to_owned),
            decided_by,
        }
    }

    async fn register<B, T>(&self, sync: &SyncCoordinator<B, T>) -> p2p::error::Result<()>
    where
        B: Blockstore + 'static,
        T: P2PTransport,
    {
        let creator = self.creator.as_deref();
        match &self.decided_by {
            DecidedBy::Own(document) => {
                sync.push_found_document_to_replicators_with_creator(
                    &self.cid,
                    &self.block,
                    &self.doc_id,
                    &self.collection_id,
                    document.as_ref(),
                    creator,
                )
                .await
            }
            DecidedBy::Parent(source) => {
                sync.push_chunk_to_replicators_with_creator(
                    &self.cid,
                    &self.block,
                    &self.doc_id,
                    &self.collection_id,
                    source.as_ref(),
                    creator,
                )
                .await
            }
            DecidedBy::Collection => {
                sync.push_to_replicators_with_creator(
                    &self.cid,
                    &self.block,
                    &self.doc_id,
                    &self.collection_id,
                    creator,
                )
                .await
            }
        }
    }
}

/// Registers the sender markers of committed heads, one error slot per head.
///
/// The registration runs as a coordinator task: a request dropped at its
/// deadline must not leave a committed head with no marker, or no peer ever
/// gets it.
pub(super) async fn register_committed_heads<B, T>(
    sync: &Arc<SyncCoordinator<B, T>>,
    heads: Vec<CommittedHead>,
) -> Vec<Option<String>>
where
    B: Blockstore + 'static,
    T: P2PTransport,
{
    if heads.is_empty() {
        return Vec::new();
    }
    let count = heads.len();
    let (done, outcome) = oneshot::channel();
    let owner = Arc::clone(sync);
    sync.spawn_background_task("register_committed_heads", async move {
        let mut errors = Vec::with_capacity(heads.len());
        for head in &heads {
            errors.push(
                head.register(&owner)
                    .await
                    .err()
                    .map(|error| error.to_string()),
            );
        }
        let _ = done.send(errors);
    });
    outcome.await.unwrap_or_else(|_| {
        vec![Some("the coordinator stopped before registering the head's markers".into()); count]
    })
}

/// Joins every error into one message, or `None` when all heads registered.
pub(super) fn joined(errors: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let errors: Vec<String> = errors.into_iter().flatten().collect();
    (!errors.is_empty()).then(|| errors.join("; "))
}
