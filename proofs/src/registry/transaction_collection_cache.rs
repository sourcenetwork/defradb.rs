use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A transaction's collection cache (#2780) keeps a shared folded map
        // and a short recent layer, so a load copies at most that layer. The
        // cache is internal to a transaction; crates/database/db/tests/collection/cache.rs
        // checks the same laws on the Rust type. Boundary.
        family: "Transaction collection cache",
        name: "add_view / remove_view / fold_view / populate_view / add_recent_bounded / add_recent_names_nodup / remove_recent_names_nodup / names_nodup / len_eq_names_length: the two layers answer every lookup as one map would, their names stay unique, their length is the view domain's size, and the layer a load copies stays bounded",
        axis: Lean,
        anchor: "crates/database/db/src/collection/cache.rs CollectionCache::get, CollectionCache::add, CollectionCache::fold, CollectionCache::remove, CollectionCache::populate",
        model_ref: "CollectionCacheLayers (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
