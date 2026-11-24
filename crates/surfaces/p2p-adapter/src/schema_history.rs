use blockstore::Blockstore;
use cid::Cid;
use defra_core::{
    merge::{BlockMetadata, MergeHandler, MergeOutcome},
    Block, CrdtDelta,
};
use rapidhash::{HashSetExt, RapidHashSet};

use crate::{P2PError, P2PErrorExt as _, P2PResult};

/// Apply fetched definition ancestors before their patches. Fetching a block
/// alone does not register the collection version it describes.
pub(crate) async fn merge_schema_history<B: Blockstore, M: MergeHandler>(
    root: Cid,
    blockstore: &B,
    handler: &M,
) -> P2PResult<()> {
    let mut pending = vec![(root, false)];
    let mut visiting = RapidHashSet::new();
    let mut merged = RapidHashSet::new();
    while let Some((cid, apply)) = pending.pop() {
        if merged.contains(&cid) {
            continue;
        }
        let data = blockstore
            .get(&cid)
            .await
            .map_err(|error| P2PError::internal(format!("read schema block {cid}: {error}")))?
            .ok_or_else(|| P2PError::not_found(format!("schema predecessor block {cid}")))?;
        let block = Block::from_dag_cbor(&data).map_err(|error| {
            P2PError::invalid_input(format!("decode schema block {cid}: {error}"))
        })?;
        if !matches!(block.delta, CrdtDelta::CollectionDefinition(_)) {
            return Err(P2PError::invalid_input(format!(
                "block {cid} is not a collection definition"
            )));
        }
        if !apply {
            if !visiting.insert(cid) {
                return Err(P2PError::invalid_input(format!(
                    "cycle in schema history at {cid}"
                )));
            }
            pending.push((cid, true));
            pending.extend(
                block
                    .heads
                    .unwrap_or_default()
                    .into_iter()
                    .rev()
                    .map(|head| (head, false)),
            );
            continue;
        }
        let outcome = handler
            .handle_block(&cid, &data, BlockMetadata::schema_sync())
            .await
            .map_err(|error| P2PError::internal(format!("merge schema block {cid}: {error}")))?;
        if !matches!(outcome, MergeOutcome::Merged) {
            return Err(P2PError::internal(format!(
                "schema block {cid} was not applied: {outcome:?}"
            )));
        }
        visiting.remove(&cid);
        merged.insert(cid);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/schema_history.rs"]
mod tests;
