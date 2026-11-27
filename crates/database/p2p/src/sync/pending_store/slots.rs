//! The durable cap: one slot per durable registration.
//!
//! The cap is `capacity` slot keys, `/p2p/pending_dag_slot/{index}`, each
//! holding the bytes of the root that occupies it. A root new to the store
//! claims a free slot in its registration transaction. It starts at a ticket
//! from `Db::allocate`, so concurrent registrations start at different slots
//! and do not meet, and probes onward; it finds the store full only after it
//! read every slot. Every slot read is validated, so a registration that saw
//! a slot free and another that took it never both commit: the engine re-runs
//! the loser, which sees the slot taken. A root that retires the older root
//! of its sender scope takes over that root's slot, and a discharged root
//! frees its slot in the transaction that deletes its record.
//!
//! A record written before slots existed holds none; there were at most the
//! old cap of them, and each leaves when its root merges or is quarantined.

use cid::Cid;
use datastore::NamespaceView;
use storage::corekv::{Result, Store};

/// The counter tickets are allocated from.
const TICKET_KEY: &[u8] = b"/p2p/pending_dag_slot_ticket";

/// Where a registration starts probing for a free slot.
pub(super) async fn ticket<S>(systemstore: &S) -> Result<u64>
where
    S: Store + ?Sized,
{
    Ok(systemstore.allocate(TICKET_KEY, 1).await?.start)
}

/// Claims a free slot for `root`, probing from `ticket`; `None` when every
/// one of the `capacity` slots is held.
pub(super) async fn claim(
    systemstore: &NamespaceView,
    root: &Cid,
    ticket: u64,
    capacity: usize,
) -> Result<Option<u64>> {
    let capacity = capacity.max(1) as u64;
    for probe in 0..capacity {
        let slot = ticket.wrapping_add(probe) % capacity;
        let key = key(slot);
        if systemstore.get(&key).await?.is_none() {
            systemstore.set(&key, &root.to_bytes()).await?;
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// Hands `slot` to `root`.
pub(super) async fn assign(systemstore: &NamespaceView, slot: u64, root: &Cid) -> Result<()> {
    systemstore.set(&key(slot), &root.to_bytes()).await
}

/// Frees `slot`.
pub(super) async fn free(systemstore: &NamespaceView, slot: u64) -> Result<()> {
    systemstore.delete(&key(slot)).await
}

fn key(slot: u64) -> Vec<u8> {
    format!("/p2p/pending_dag_slot/{slot}").into_bytes()
}
