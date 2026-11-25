use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query, db, storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Explicit transaction statement atomicity",
        name: "INV_AbortedEffectsRemoved / INV_SuccessfulEffectsRetained / INV_NoCommitWhileCleanupPending",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/mutation.rs explicit transaction mutation execution; crates/database/db/src/txn/mod.rs statement savepoint; crates/engine/storage/src/backends/regolith/transaction.rs savepoint callbacks",
        model_ref: "MC_StatementAtomicity_Green.cfg / MC_StatementAtomicity_Red_*.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
