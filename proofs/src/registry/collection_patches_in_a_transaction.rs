use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, http, cli";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Collection patches in a transaction",
        name: "INV_IsolatedUntilCommit / INV_NoTraceAfterRollback / INV_SeesOwnPatch / INV_FailedPatchLeavesNothing / INV_FollowUpRanWhenCommitReturns / INV_NoLostPatch",
        axis: Tla,
        anchor: "crates/database/db/src/txn/registry/lifecycle.rs patch_collection_as, commit_error; crates/database/db/src/txn/registry/patches.rs DbTransactionRegistry::patch_collection_in_txn_as; crates/database/db/src/txn/registry/mod.rs finish_statement; crates/database/db/src/definition/patch/mod.rs DB::patch_collection, DB::patch_collection_with_txn; crates/database/db/src/definition/patch/outcome.rs DB::after_patch, forget_cached_collections; crates/database/db/src/definition/patch/store.rs DB::store_new_version; crates/database/db/src/definition/patch/in_place.rs DB::update_version_in_place; crates/database/db/src/definition/patch/added.rs DB::store_added_version",
        model_ref: "MC_PatchInTxn_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
