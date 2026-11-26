/-!
Adding a replicator records the time it was added. Updating its collections
must keep the existing status-change time; a recovered status can use a zero
value, which an update must also preserve.

Rust owner: crates/surfaces/p2p-adapter/src/replicator_install_store.rs
Prepare::run and crates/database/p2p/src/bitswap/registry.rs
ReplicatorRegistry::set_replicator_info.
-/
namespace ReplicatorAddedAt

-- A new record uses the supplied time; an update keeps the previous one.
def timestamp (previous : Option Nat) (now : Nat) : Nat := previous.getD now

-- Creating a record uses the caller's current time.
theorem create_uses_now (now : Nat) : timestamp none now = now := rfl

-- Updating a record does not change its stored timestamp.
theorem update_keeps_previous (old now : Nat) : timestamp (some old) now = old := rfl

-- A zero timestamp from a recovered peer also stays zero on update.
theorem recovered_zero_stays_zero (now : Nat) : timestamp (some 0) now = 0 := rfl
end ReplicatorAddedAt
