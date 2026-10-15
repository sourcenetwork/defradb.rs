use cid::Cid;
use defra_core::block::{Block, CrdtDelta};
use defra_core::merge::RecoveredBlockMetadata;
use storage::corekv::{Key, Store};
use storage::keys::systemstore::CollectionKey;

use super::{DbMergeHandler, MergeError};

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    pub(super) async fn recover_metadata_from_block(
        &self,
        cid: &Cid,
        block_data: &[u8],
    ) -> Result<Option<RecoveredBlockMetadata>, MergeError> {
        let block =
            Block::from_dag_cbor(block_data).map_err(|e| MergeError::BlockDecode(e.to_string()))?;

        // Deltas carry no document identity: recover it from the ownership
        // index (composites can also derive it from their DAG).
        let doc_id = match &block.delta {
            CrdtDelta::Composite(_) => match self.resolve_composite_doc_id(cid, &block, 0).await {
                Ok(doc_id) => doc_id,
                // No recoverable identity → treat as unrecoverable metadata;
                // real infrastructure errors propagate.
                Err(MergeError::MergeFailed(_)) => return Ok(None),
                Err(e) => return Err(e),
            },
            CrdtDelta::Lww(_) | CrdtDelta::Counter(_) => {
                match self.resolve_field_block_doc_id(cid).await? {
                    Some(doc_id) => doc_id,
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        let Some(collection_id) = block.delta.schema_version_id().map(ToString::to_string) else {
            return Ok(None);
        };

        let Some(creator) = self.verify_block_signature(cid, &block, block_data).await? else {
            return Ok(None);
        };

        Ok(Some(
            RecoveredBlockMetadata::new(doc_id, collection_id, creator.clone())
                .with_verified_creator(Some(creator)),
        ))
    }

    /// Resolve a predecessor from its persisted definition, never a partial block.
    pub(crate) async fn resolve_previous_collection_version(
        &self,
        block: &Block,
    ) -> Result<Option<schema::CollectionVersion>, MergeError> {
        let Some(heads) = &block.heads else {
            return Ok(None);
        };
        let txn = self.db.new_txn(true).await?;
        let store = txn.systemstore()?;
        for head in heads {
            let key = CollectionKey::new(head.to_string());
            let Some(data) = store
                .get(&key.bytes())
                .await
                .map_err(|error| MergeError::Storage(error.to_string()))?
            else {
                continue;
            };
            let previous: schema::CollectionVersion = serde_json::from_slice(&data)
                .map_err(|error| MergeError::Storage(error.to_string()))?;
            if !previous.is_placeholder {
                return Ok(Some(previous));
            }
        }
        Ok(None)
    }
}
