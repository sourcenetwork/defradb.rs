//! The merged blocks a DAG fetch names to its provider, so the provider can
//! leave out the history this node already holds.

use blockstore::Blockstore;
use cid::Cid;
use rapidhash::{HashSetExt, RapidHashSet};

use super::SyncCoordinator;
use crate::sync::car::MAX_HAVE_CIDS;
use crate::transport::P2PTransport;

/// Ancestor levels named in full; past them only levels at powers of two.
const DENSE_LEVELS: usize = 4;

/// The deepest ancestor level read. A provider that shares no named block
/// with this node falls back to its depth cut.
const MAX_LEVEL: usize = 128;

/// A document's composite heads and some of their ancestors, newest first,
/// up to [`MAX_HAVE_CIDS`]: the heads and the next three levels of parents,
/// then the levels 4, 8, 16 … [`MAX_LEVEL`] composites back. A head is
/// written only once its DAG is complete here, so every block on this list
/// has its whole closure held. A provider that has not seen this node's
/// newest updates still finds a block both hold; the doubling gaps bound what
/// it sends again to about the depth this node diverged by.
pub(crate) async fn haves_from_heads<B>(blockstore: &B, heads: Vec<Cid>) -> Vec<Cid>
where
    B: Blockstore,
{
    let mut haves = Vec::with_capacity(MAX_HAVE_CIDS);
    let mut seen = RapidHashSet::with_capacity(MAX_HAVE_CIDS);
    let mut level_cids = heads;
    for level in 0..=MAX_LEVEL {
        if level_cids.is_empty() || haves.len() >= MAX_HAVE_CIDS {
            break;
        }
        let named = level < DENSE_LEVELS || level.is_power_of_two();
        let mut parents = Vec::new();
        for cid in level_cids {
            if !seen.insert(cid) {
                continue;
            }
            let Ok(Some(data)) = blockstore.get(&cid).await else {
                continue;
            };
            if named && haves.len() < MAX_HAVE_CIDS {
                haves.push(cid);
            }
            if let Ok(block) = defra_core::Block::from_dag_cbor(&data) {
                parents.extend(block.heads.unwrap_or_default());
            }
        }
        level_cids = parents;
    }
    haves
}

impl<B: Blockstore + 'static, T: P2PTransport> SyncCoordinator<B, T> {
    /// The have list for a fetch of `doc_id`; empty when this node holds none
    /// of the document, or the fetch is not for one document.
    pub(crate) async fn document_haves(&self, doc_id: &str) -> Vec<Cid> {
        if doc_id.is_empty() {
            return Vec::new();
        }
        let heads = self
            .subscriptions
            .head_provider
            .get_document_heads(doc_id)
            .await
            .unwrap_or_default();
        haves_from_heads(self.manager.blockstore().as_ref(), heads).await
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/haves_spacing.rs"]
mod tests;
