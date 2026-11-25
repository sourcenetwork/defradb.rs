use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Iroh peer identity cache",
        name: "Concurrent inserts leave every cached peer with exactly one place in the eviction line, and the map within capacity once they settle",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/identity_cache.rs IrohPeerIdentityCache::insert, IrohPeerIdentityCache::enqueue",
        model_ref: "MC_PeerIdentityCache_{Green,Green_One,Green_Three,Red_NoEvict,Red_RefreshEnqueues,Red_EnqueueFirst}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/iroh/identity_cache.rs concurrent_inserts_settle_with_one_place_per_cached_peer",
    },
    Property {
        family: "Iroh peer identity cache",
        name: "PeerIdentityCache.refresh_keeps_order / insert_bounded / overflow_evicts_front / insert_nodup / inserted_is_cached",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/identity_cache.rs IrohPeerIdentityCache::insert, IrohPeerIdentityCache::enqueue",
        model_ref: "PeerIdentityCache (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/iroh/identity_cache.rs refreshing_a_peer_keeps_its_single_slot, a_refresh_keeps_its_first_insert_place_in_eviction_order, a_peer_inserted_again_after_eviction_takes_a_new_place",
    },
];
