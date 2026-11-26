use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #2917: a commit that publishes a collection version naming a lens
        // owes the lens first, in the same ordered publication, and the
        // registration after it clears the owed entry only once the lens is
        // in the store, so no request that sees the version misses its
        // transform.
        family: "Lens with its version",
        name: "INV_NoMissingTransform / INV_VisibleLensKnown",
        axis: Tla,
        anchor: "crates/database/db/src/definition/migration/committed_lenses.rs DB::owe_lens_on_commit; crates/database/db/src/definition/patch/store.rs DB::store_new_version; crates/database/db/src/txn/registry/schemas.rs DbTransactionRegistry::set_migration_in_txn_as",
        model_ref: "MC_LensWithVersion_{Green,Red_NoOwe,Red_OweAfterPublish,Red_ClearFirst}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
