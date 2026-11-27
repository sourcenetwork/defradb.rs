use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #2917: a commit that stores a collection version naming a lens
        // holds its compiled copy under a ticket of its own from just before
        // the commit lands, owes the lens in its ordered publication before
        // letting go, and lets go of its own ticket alone when it fails; the
        // registration clears the owed entry only once the lens is in the
        // store. No request that sees the version, in storage or in the
        // cache, misses its transform, and a failed commit leaves nothing.
        family: "Lens with its version",
        name: "INV_NoMissingTransform / INV_VisibleLensKnown / INV_FailedCommitLetsGo / INV_OwedOnlyCommitted",
        axis: Tla,
        anchor: "crates/database/db/src/definition/migration/committed_lenses.rs DB::owe_lens_on_commit, CommittedLensStore::hold, let_go, held, owe, run, transform_doc; crates/database/db/src/definition/patch/store.rs DB::store_new_version; crates/database/db/src/txn/registry/schemas.rs DbTransactionRegistry::set_migration_in_txn_as",
        model_ref: "MC_LensWithVersion_{Green,Red_NoHold,Red_SharedHold,Red_LetGoFirst,Red_KeepOnAbort,Red_ClearFirst,Red_OwedFirst}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
