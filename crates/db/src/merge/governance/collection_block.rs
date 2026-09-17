use defra_core::block::CollectionDeltaPayload;
use defra_core::merge::{BlockMetadata, MergeOutcome};
use storage::corekv::Store;

use crate::merge::merge_handler::{DbMergeHandler, MergeError};

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// The rejection for a collection block naming a governed collection, or
    /// `None` when the collection is ungoverned and the block merges normally.
    ///
    /// A collection block is not a composite, so the validator never judges
    /// one, yet merging it installs a head in the collection's verifiable
    /// history and drives the documents it links into the merge path. Its
    /// links are skipped when this node does not hold them, so a peer can
    /// install a head by sending the block alone. A governed collection
    /// therefore has no collection blocks at all: it is not branchable, and
    /// one arriving over replication is refused on its content.
    pub(crate) async fn refuse_governed_collection_block(
        &self,
        payload: &CollectionDeltaPayload,
        metadata: &BlockMetadata<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        let Some(collection) = self
            .block_collection(&payload.schema_version_id, metadata.collection_id)
            .await?
        else {
            return Ok(None);
        };
        if !self.is_governed(collection.schema()) {
            return Ok(None);
        }
        Ok(Some(MergeOutcome::rejected(format!(
            "collection {} is governed, so it is not branchable and merges no collection blocks",
            collection.name()
        ))))
    }
}
