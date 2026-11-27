//! Where a merge walk's entered frame gets its block. The block a merge is
//! asked for arrives decoded; a parent named in a child's `heads` is loaded
//! from the blockstore when the walk reaches it. The collection walk and the
//! composite walk share this, so a parent frame always names its child and
//! no frame is left without a source.

use super::*;

// The frames that carry a source sit beside exit frames that hold a block and a
// payload by value, so boxing the root would add an allocation per merge and
// shrink nothing.
#[allow(clippy::large_enum_variant)]
pub(super) enum FrameSource<P> {
    /// The walk's one root.
    Root(Block, P),
    Parent {
        child_cid: Cid,
    },
}

impl<P> FrameSource<P> {
    /// The root frame of a walk over `block`, merging `payload`.
    pub(super) fn root(block: &Block, payload: &P) -> Self
    where
        P: Clone,
    {
        Self::Root(block.clone(), payload.clone())
    }

    pub(super) fn is_root(&self) -> bool {
        matches!(self, Self::Root(..))
    }
}

/// The delta payload a walk merges, read off a loaded parent block.
pub(super) trait FramePayload: Clone {
    fn of(delta: &CrdtDelta) -> Option<&Self>;
}

impl FramePayload for defra_core::block::CollectionDeltaPayload {
    fn of(delta: &CrdtDelta) -> Option<&Self> {
        match delta {
            CrdtDelta::Collection(payload) => Some(payload),
            _ => None,
        }
    }
}

impl FramePayload for defra_core::block::CompositeDeltaPayload {
    fn of(delta: &CrdtDelta) -> Option<&Self> {
        match delta {
            CrdtDelta::Composite(payload) => Some(payload),
            _ => None,
        }
    }
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// The block and payload an entered frame merges, or `None` for a parent
    /// that is not held, does not decode, or carries another kind of delta:
    /// the walk skips such a parent.
    pub(super) async fn frame_block<P>(
        &self,
        cid: &Cid,
        source: FrameSource<P>,
    ) -> Option<(Block, P)>
    where
        P: FramePayload,
    {
        let child_cid = match source {
            FrameSource::Root(block, payload) => return Some((block, payload)),
            FrameSource::Parent { child_cid } => child_cid,
        };
        let data = match self.blockstore.get(cid).await {
            Ok(Some(data)) => data,
            Ok(None) => {
                tracing::debug!(
                    parent_cid = %cid,
                    %child_cid,
                    "Parent block not in blockstore, skipping"
                );
                return None;
            }
            Err(error) => {
                tracing::debug!(
                    parent_cid = %cid,
                    %child_cid,
                    %error,
                    "Failed to load parent block, skipping"
                );
                return None;
            }
        };
        let block = Block::from_dag_cbor(&data).ok()?;
        let payload = P::of(&block.delta)?.clone();
        Some((block, payload))
    }
}
