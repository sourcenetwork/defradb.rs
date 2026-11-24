use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "defra-core, storage, db, query, http, cli";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Commit outcome classification",
        name: "INV_AbortedMeansNothingLanded / INV_CommittedMeansLanded / INV_RefusalIsAborted",
        axis: Tla,
        anchor: "crates/engine/storage/src/backends/regolith/transaction.rs commit_failure refused_before_apply; crates/engine/storage/src/corekv/errors.rs; crates/database/db/src/txn/owner/mod.rs; crates/database/db/src/txn/registry/lifecycle.rs commit_error commit_unavailable; crates/surfaces/http/src/handlers/graphql/transactions.rs commit_error_response; crates/entry-points/cli/src/commands/client/http_client/mod.rs HttpClient::extract_error HttpClient::send_error",
        model_ref: "MC_CommitOutcome_{Green,Red_FailureAborted,Red_RetainAborted,Red_NotFoundAborted,Red_RefusalIndeterminate,Red_LostReplyAborted}.cfg",
        tiers: &[Contract],
        bound_by: "tools/integration-test/tests/testo/txn/commit_outcome.rs an_injected_io_failure_at_commit_reports_indeterminate a_commit_on_a_finished_transaction_reports_retained_status a_conflict_refusal_leaves_nothing_and_reports_aborted; crates/database/db/src/txn/owner/tests.rs a_commit_that_may_have_landed_leaves_a_final_indeterminate_outcome",
    },
    Property {
        family: "Commit outcome classification",
        name: "CommitOutcome.round_trip / asStr_injective / exitCode_injective / landed_is_never_aborted / refusal_is_aborted",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/commit_outcome.rs; crates/engine/storage/src/backends/regolith/transaction.rs commit_failure",
        model_ref: "CommitOutcome.lean",
        tiers: &[Contract],
        bound_by: "crates/foundation/defra-core/src/commit_outcome.rs every_class_round_trips_through_its_wire_value; crates/engine/storage/src/backends/regolith/transaction.rs only_a_commit_failure_that_may_have_landed_is_indeterminate",
    },
];
