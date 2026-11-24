//! Explicit peer/root serving authority. This is not subscription or retry state.
//! Grants survive cache eviction, elapsed time and restart until explicitly
//! revoked. Removing blocks makes them unavailable, but does not erase the grant.

use super::Peerstore;
use crate::corekv::{
    Error, Reader, Result, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryTxn, Store,
};

const PREFIX: &[u8] = b"/selected-car/v1/";
const GRANTED: &[u8] = b"DFRCAR1";

fn key(peer: &str, root: &[u8]) -> Result<Vec<u8>> {
    if peer.is_empty() || root.is_empty() {
        return Err(Error::Other(
            "selected CAR authority requires peer and root".into(),
        ));
    }
    let mut key = Vec::with_capacity(PREFIX.len() + 8 + peer.len() + root.len());
    key.extend_from_slice(PREFIX);
    key.extend_from_slice(&(peer.len() as u64).to_be_bytes());
    key.extend_from_slice(peer.as_bytes());
    key.extend_from_slice(root);
    Ok(key)
}

struct Change {
    key: Vec<u8>,
    grant: bool,
}

impl RetryAttemptFn for Change {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            // Grants and revocations of one scope settle in native commit order.
            let _ = txn.get(&self.key).await?;
            if self.grant {
                txn.set(&self.key, GRANTED).await?;
            } else {
                txn.delete(&self.key).await?;
            }
            Ok(Box::new(()) as RetryAttemptValue)
        })
    }
}

impl<S> Peerstore<S>
where
    S: Store + 'static,
{
    /// Commit permission for exactly this peer and root before its head hint.
    /// Only a new explicit grant can restore permission after revocation.
    pub async fn grant_selected_car_root(&self, peer: &str, root: &[u8]) -> Result<()> {
        self.transact_conflicts(Box::new(Change {
            key: key(peer, root)?,
            grant: true,
        }))
        .await
    }

    /// Revoke future CAR reads of this selected root, independently of subscriptions.
    /// Already-sent bytes cannot be retracted. Overlapping explicit grants and
    /// revocations are ordered by their engine commits.
    pub async fn revoke_selected_car_root(&self, peer: &str, root: &[u8]) -> Result<()> {
        self.transact_conflicts(Box::new(Change {
            key: key(peer, root)?,
            grant: false,
        }))
        .await
    }

    /// A fresh, write-free check of durable exact-scope serving authority.
    pub async fn selected_car_root_allowed(&self, peer: &str, root: &[u8]) -> Result<bool> {
        let txn = self.store.new_txn(true).await?;
        match txn.get(&key(peer, root)?).await? {
            None => Ok(false),
            Some(value) if value.as_ref() == GRANTED => Ok(true),
            Some(_) => Err(Error::Other("invalid selected CAR authority record".into())),
        }
    }
}

#[cfg(test)]
mod tests;
