use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, storage (pinned regolith)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Commit-ordered document arrivals",
        name: "dense positions / commit order / at-most-once / snapshot prefix",
        axis: Tla,
        anchor: "crates/database/db/src/event/arrivals.rs record; crates/engine/storage/src/backends/regolith/transaction.rs append; crates/database/db/tests/read/arrival_transactions.rs",
        model_ref: "regolith@055589006356738a31ee3b0e965d8e2a9c5bc057 proofs/tla/CommitOrderedAppend.tla (GREEN Groups/Pipeline/Faults, RED Counter/OnceFromView/AssignBeforeValidation)",
        tiers: &[Behavioral],
        bound_by: "crates/database/db/tests/read/arrival_transactions.rs reversed_concurrent_commits_receive_cursors_in_commit_order, rollback_is_atomic_and_duplicate_arrivals_do_not_advance, cursor_survives_database_reopen",
    },
];
