//! The order an owed transfer keeps on its object
//! (`proofs/tla/OwnerTransfer.tla`, `ObjectOrder`;
//! `proofs/lean/OwnerTransferOrder.lean`).
//!
//! The provider takes a change only from whoever the object's owner is when
//! it arrives. A transfer is sent once every earlier change on its object is
//! decided, so the changes its actor made as the owner reach the provider
//! while it still is; and a later change on the object is sent once the
//! transfer is decided, so it reaches the provider under the owner it was
//! made for. A change parked as refused is decided and holds nothing back;
//! one waiting for authority still holds, since it is sent once its actor
//! hands in a fresh credential, unless nobody checked its actor at staging
//! (it holds nothing on its own tuple either). The changes of one request are applied in
//! their queue order, so they hold nothing back among themselves.

use rapidhash::RapidHashSet;
use storage::corekv::Store;

use super::ExternalDocumentAcp;
use crate::error::Result;
use crate::outbox::{OwedChange, OwedChanges, OwedOp, OwedState};

/// One change owed on the same object, earlier in the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Earlier {
    pub(crate) seq: u64,
    /// It is a transfer of the object's ownership.
    pub(crate) transfer: bool,
    /// It is still to be delivered: staged, in flight or waiting for
    /// authority. A change parked as refused is not, and neither is one
    /// staged unchecked that waits for authority.
    pub(crate) live: bool,
}

/// Whether a change (a transfer when `transfer` holds) waits behind one of
/// `earlier`, sharing a request with `members`: an earlier live change
/// outside the request that is a transfer, or any such change when this one
/// is a transfer.
pub(crate) fn held_by_transfer(
    transfer: bool,
    earlier: &[Earlier],
    members: &RapidHashSet<u64>,
) -> bool {
    first_holder(transfer, earlier, members).is_some()
}

/// The first of `earlier` that holds the change back, as
/// [`held_by_transfer`] decides.
fn first_holder(transfer: bool, earlier: &[Earlier], members: &RapidHashSet<u64>) -> Option<u64> {
    earlier
        .iter()
        .find(|e| e.live && !members.contains(&e.seq) && (transfer || e.transfer))
        .map(|e| e.seq)
}

/// The changes owed on `change`'s object before it, as `changes` sees them.
pub(super) async fn earlier_on_object(
    changes: &OwedChanges,
    change: &OwedChange,
) -> Result<Vec<Earlier>> {
    let tuple = &change.tuple;
    let mut earlier = Vec::new();
    for seq in changes
        .live_on(&tuple.policy_id, &tuple.resource, &tuple.object)
        .await?
    {
        if seq >= change.seq {
            break;
        }
        if let Some(owed) = changes.get(seq).await? {
            // A change nobody vouched for that waits for authority holds
            // nothing, as it holds nothing on its own tuple.
            let at_rest = match owed.state {
                OwedState::Refused { .. } => true,
                OwedState::AwaitingAuthority => !owed.authority_checked,
                OwedState::Staged => false,
            };
            earlier.push(Earlier {
                seq,
                transfer: owed.op == OwedOp::Transfer,
                live: !at_rest,
            });
        }
    }
    Ok(earlier)
}

impl<S> ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    /// The change on `change`'s object that the order around a transfer
    /// puts before it, as committed now, outside the request `members`
    /// (see [`held_by_transfer`]).
    pub(super) async fn holder_on_object(
        &self,
        change: &OwedChange,
        members: &RapidHashSet<u64>,
    ) -> Result<Option<OwedChange>> {
        let txn = self.begin(true).await?;
        let changes = self.owed_changes(&txn.acpstore(), crate::outbox::StagedCount::new());
        let holder = match earlier_on_object(&changes, change).await {
            Ok(earlier) => match first_holder(change.op == OwedOp::Transfer, &earlier, members) {
                Some(seq) => changes.get(seq).await,
                None => Ok(None),
            },
            Err(error) => Err(error),
        };
        drop(changes);
        txn.discard()
            .map_err(|error| crate::Error::Storage(error.to_string()))?;
        holder
    }
}

#[cfg(test)]
mod tests {
    use rapidhash::HashSetExt;

    use super::*;

    fn earlier(seq: u64, transfer: bool, live: bool) -> Earlier {
        Earlier {
            seq,
            transfer,
            live,
        }
    }

    #[test]
    fn a_transfer_waits_for_every_earlier_live_change_and_a_change_for_an_earlier_transfer() {
        let none = RapidHashSet::new();
        let grant = earlier(1, false, true);
        let transfer = earlier(2, true, true);
        assert!(held_by_transfer(true, &[grant], &none));
        assert!(held_by_transfer(false, &[transfer], &none));
        assert!(!held_by_transfer(false, &[grant], &none));
        assert!(!held_by_transfer(true, &[], &none));
    }

    #[test]
    fn a_refused_change_and_a_change_of_the_same_request_hold_nothing() {
        let refused = earlier(1, true, false);
        assert!(!held_by_transfer(false, &[refused], &RapidHashSet::new()));
        assert!(!held_by_transfer(true, &[refused], &RapidHashSet::new()));
        let mut members = RapidHashSet::new();
        members.insert(1);
        assert!(!held_by_transfer(
            true,
            &[earlier(1, false, true)],
            &members
        ));
        assert!(!held_by_transfer(
            false,
            &[earlier(1, true, true)],
            &members
        ));
    }
}
