//! A replicator's stored record narrowed in the transaction that reads it.

use storage::corekv::{
    self, Key, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryOptions, RetryTxn, Store,
};
use storage::keys::peerstore::{ReplicatorIncarnationKey, ReplicatorKey};
use storage::stores::peerstore::{decode_retry_marker_generation, encode_retry_marker_generation};

use crate::{P2PError, P2PErrorExt as _, P2PResult};

#[cfg(test)]
#[path = "../tests/unit/replicator_persist.rs"]
mod tests;

fn storage_error(error: impl std::fmt::Display) -> corekv::Error {
    corekv::Error::Other(error.to_string())
}

/// `stored` without the collections `kept` leaves out, with their filters and
/// chunk pairs. It never gains a collection: one that is only in memory has no
/// filter here, so writing it would replicate it unfiltered after a restart.
pub(crate) fn narrowed(mut stored: p2p::ReplicatorInfo, kept: &[String]) -> p2p::ReplicatorInfo {
    let removed: Vec<String> = stored
        .collections
        .iter()
        .filter(|id| !kept.contains(id))
        .cloned()
        .collect();
    for collection_id in &removed {
        stored.remove_collection(collection_id);
    }
    stored
}

enum Outcome {
    Done,
    Reallocate,
}

struct Narrow {
    peer: String,
    kept: Vec<String>,
    incarnation: u64,
}

impl RetryAttemptFn for Narrow {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let incarnation_key = ReplicatorIncarnationKey::new(&self.peer).bytes();
            let current = txn
                .get(&incarnation_key)
                .await?
                .map(|bytes| {
                    decode_retry_marker_generation(&bytes)
                        .ok_or_else(|| storage_error("invalid peer incarnation"))
                })
                .transpose()?
                .unwrap_or(0);
            if current > self.incarnation {
                return Ok(Box::new(Outcome::Reallocate) as RetryAttemptValue);
            }
            let row_key = ReplicatorKey::new(&self.peer).bytes();
            let Some(bytes) = txn.get(&row_key).await? else {
                return Ok(Box::new(Outcome::Done) as RetryAttemptValue);
            };
            // An unreadable record's filters are unknown; rewriting it would drop them.
            let stored = p2p::ReplicatorInfo::from_bytes(&bytes).map_err(storage_error)?;
            let info = narrowed(stored, &self.kept);
            txn.set(&row_key, &info.to_bytes().map_err(storage_error)?)
                .await?;
            txn.set(
                &incarnation_key,
                &encode_retry_marker_generation(self.incarnation),
            )
            .await?;
            Ok(Box::new(Outcome::Done) as RetryAttemptValue)
        })
    }
}

/// Narrows `peer`'s stored record to `kept`, as [`narrowed`] does. A peer
/// with no stored record is left without one.
pub(crate) async fn narrow<S>(
    store: &S,
    retry: RetryOptions,
    peer: &str,
    kept: &[String],
) -> P2PResult<()>
where
    S: Store,
{
    loop {
        let incarnation = store
            .allocate(b"/rep/incarnation/sequence", 1)
            .await
            .map_err(|error| {
                P2PError::persistence(format!("failed to persist replicator: {error}"))
            })?
            .end;
        let outcome: Outcome = corekv::transact(
            store,
            retry,
            None,
            Box::new(Narrow {
                peer: peer.to_owned(),
                kept: kept.to_vec(),
                incarnation,
            }),
        )
        .await
        .map_err(|error| {
            P2PError::persistence(format!(
                "failed to persist replicator: {}",
                error.into_storage()
            ))
        })?;
        if let Outcome::Done = outcome {
            return Ok(());
        }
    }
}
