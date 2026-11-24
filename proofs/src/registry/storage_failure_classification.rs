use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Storage failure classification",
        name: "ClassificationPreserved / NoReplayAuthority",
        axis: Tla,
        anchor: "crates/engine/storage/src/corekv/errors.rs; crates/database/db/src/error/mod.rs; crates/engine/query/src/runner/turn.rs; crates/engine/query/src/executor.rs; crates/surfaces/http/src/error.rs",
        model_ref: "MC_StorageFailure_Green.cfg / MC_StorageFailure_Red_LostCause.cfg / MC_StorageFailure_Red_Text.cfg / MC_StorageFailure_Red_Retry.cfg",
        tiers: &[Contract],
        bound_by: "crates/engine/query/tests/storage_failure.rs storage_failure_classification_matches_executed_model",
    },
    Property {
        family: "Storage failure classification",
        name: "StorageFailure.preserves / full_code / other_code / opaque_code",
        axis: Lean,
        anchor: "crates/engine/storage/src/corekv/io_failure.rs; crates/engine/storage/src/backends/regolith/transaction.rs map_txn_error; crates/engine/query/src/executor.rs QueryResponseError::from_query_error; crates/engine/query/tests/storage_failure.rs",
        model_ref: "StorageFailure.lean",
        tiers: &[Contract],
        bound_by: "crates/engine/query/tests/storage_failure.rs storage_failure_classification_matches_executed_model",
    },
];
