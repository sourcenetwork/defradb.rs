use std::collections::HashSet;

use cid::Cid;
use events::Subscription;
use tokio::time::{timeout_at, Instant};

use crate::{P2PError, P2PErrorExt as _, P2PResult};

pub(super) async fn wait_for_heads<B: blockstore::Blockstore>(
    blockstore: &B,
    sub: &mut Subscription,
    collection_id: &str,
    pending: HashSet<Cid>,
    deadline: Instant,
) -> P2PResult<()> {
    let mut pending: Vec<_> = pending.into_iter().collect();
    timeout_at(deadline, async {
        loop {
            let mut index = 0;
            while index < pending.len() {
                if blockstore.is_merged(&pending[index]).await.map_err(|error| {
                    P2PError::transport(format!("failed to check branchable sync head: {error}"))
                })? {
                    pending.swap_remove(index);
                } else {
                    index += 1;
                }
            }
            if pending.is_empty() {
                return Ok(());
            }
            // Events are wakeups, not proof of completion: ancestor merges may
            // emit no event for this CID, and a full subscriber queue can drop one.
            tokio::select! {
                message = sub.recv() => {
                    if message.is_none() {
                        return Err(P2PError::transport(
                            "event bus closed while syncing branchable collection",
                        ));
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
            }
        }
    })
    .await
    .map_err(|_| P2PError::transport(format!(
        "timeout while syncing branchable collection {collection_id}: completion not confirmed for {} heads",
        pending.len()
    )))?
}

#[cfg(test)]
#[path = "../../tests/libp2p/branchable_sync.rs"]
mod tests;
