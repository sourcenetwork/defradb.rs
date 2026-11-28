use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "storage, db, query, http (#3265; [guarantees](../docs/book/src/txn/guarantees.md))";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #3265: what a unique index guarantees to writers racing on one
        // node. The race is driven through the node's clients by testo and
        // on the worker pool by the db race tests; regolith's validation of
        // the entry read (v0.2.5, RepeatableRead.tla MC_DefraLevel_* configs,
        // and commit_nowait validating as commit, regolith:src/transaction/nowait.rs:1-6)
        // is assumed, not modeled. INV_RefusedOnlyOverItsOwnValue is the
        // house rule that a query read decides nothing; every Green config
        // checks it, and Red_FilterReadDecides and Red_FilterReadDecidesNil
        // plant the validated unique-eq filter read #3379 removed.
        family: "Client transaction isolation",
        name: "INV_AtMostOnePerNonNilValue / INV_LoserLeavesNothing / INV_LoserRefusedTyped / INV_NoIndexNotRefused / INV_NilNotRefused / INV_RefusedOnlyOverItsOwnValue",
        axis: Tla,
        anchor: "crates/engine/storage/src/index/unique.rs UniqueIndex::save, UniqueIndex::has_never_unique_field; crates/engine/storage/src/index/eq_iterator.rs ExactMatchIterator::new_unique; crates/database/db/src/error/mod.rs index_write_query_error; crates/database/db/src/write/autocommit/helpers.rs; crates/engine/query/src/executor.rs TXN_CONFLICT_MESSAGE, QueryResponseError::from_query_error",
        model_ref: "MC_ClientUniqueAdmission_{Green_*,Red_*}.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/unique_isolation.rs explicit_loser_is_refused_and_leaves_nothing, autocommit_loser_is_refused_as_a_unique_violation, without_an_index_both_commit; tools/integration-test/tests/testo/txn/unique_isolation.rs the_second_commit_of_a_unique_value_is_a_conflict, the_losers_rerun_is_refused_with_the_unique_code, nil_values_are_not_refused, without_an_index_both_commit, a_nil_writers_filter_for_nil_decides_nothing; tools/integration-test/tests/testo/txn/unique_filter_read.rs a_unique_eq_filter_read_does_not_fail_the_commit, a_nil_writers_unique_filter_read_does_not_fail_the_commit; tools/integration-test/tests/testo/index/unique_refusal_code.rs a_second_autocommit_create_of_a_value_carries_its_code; crates/database/db/tests/write/conflict_reasons.rs a_create_that_loses_a_unique_index_race_reports_the_violation, every_round_of_a_free_unique_race_has_one_winner_and_one_typed_refusal",
    },
    Property {
        family: "Client transaction isolation",
        name: "UniqueRefusal.code_injective / message_injective / autocommit_loser_is_unique / explicit_loser_is_conflict",
        axis: Lean,
        anchor: "crates/engine/query/src/executor.rs TXN_CONFLICT_MESSAGE, UNIQUE_CONSTRAINT_VIOLATION_ERROR_CODE, QueryResponseError::from_query_error; crates/engine/storage/src/corekv/errors.rs UNIQUE_CONSTRAINT_VIOLATION_MESSAGE",
        model_ref: "UniqueRefusal (lake build)",
        tiers: &[Contract],
        bound_by: "proofs/tests/lean_conformance.rs lean_refusal_vocab_matches_rust",
    },
    Property {
        family: "Client transaction isolation",
        name: "regolith validates the unique entry read at commit, under commit and commit_nowait alike",
        axis: Tla,
        anchor: "crates/engine/storage/src/index/unique.rs UniqueIndex::save txn.has; crates/engine/storage/src/backends/regolith/transaction.rs RegolithTxn::commit",
        model_ref: "regolith@055589006356738a31ee3b0e965d8e2a9c5bc057 proofs/tla/RepeatableRead.tla (MC_DefraLevel_* configurations); regolith:src/transaction/nowait.rs:1-6",
        tiers: &[Boundary],
        bound_by: "",
    },
];
