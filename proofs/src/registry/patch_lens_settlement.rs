use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #3052: a patch commits its version, registers any lens it staged,
        // and only then settles a reindex owed by that patch.
        family: "Patch lens settlement",
        name: "INV_SettleAfterLens",
        axis: Tla,
        anchor: "crates/database/db/src/definition/patch/mod.rs DB::patch_collection; crates/database/db/src/definition/patch/in_place.rs DB::update_version_in_place; crates/database/db/src/definition/migration/reindex/page.rs ReindexPage::walk",
        model_ref: "MC_LensSettlement_{Green,Red_Early}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
