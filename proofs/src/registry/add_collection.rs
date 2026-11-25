use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Add collection",
        name: "INV_NoCollectionUnlessAllChecksPass / INV_AllOrNone",
        axis: Tla,
        anchor: "crates/database/db/src/collection/ops/add/mod.rs DB::add_collection, prepare_new_collections, check_policies, check_collection_patch; crates/database/db/src/txn/registry/schemas.rs add_collection_in_txn_as; crates/database/db/src/txn/registry/views.rs DbTransactionRegistry::add_view_in_txn; crates/database/db/src/collection/ops/create.rs DB::create_collections_atomic, DB::stage_new_collections",
        model_ref: "MC_AddCollection_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
