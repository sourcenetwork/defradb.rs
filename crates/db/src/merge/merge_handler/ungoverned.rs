//! The merge path's governance seams when the node is built without
//! `governed-collections`: no validator judges a composite, so nothing is
//! deferred, nothing awaits a release and there is nothing to re-drive.

use cid::Cid;
use defra_core::block::{Block, CompositeDeltaPayload};
use defra_core::merge::{BlockMetadata, MergeOutcome};
use storage::corekv::Store;

use super::{DbMergeHandler, MergeError};

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    pub(crate) async fn release_merged_composite(&self, _cid: &Cid, _block: Option<&Block>) {}

    /// An unidentifiable document is an error: only a validator defers it.
    pub(crate) async fn defer_unresolved_document(
        &self,
        _cid: &Cid,
        _block: &Block,
        _payload: &CompositeDeltaPayload,
        _metadata: &BlockMetadata<'_>,
        error: MergeError,
    ) -> Result<MergeOutcome, MergeError> {
        Err(error)
    }

    pub(crate) async fn redrive_deferred(&self) {}
}
