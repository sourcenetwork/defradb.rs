use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, db-merge";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // No-loss / no-double-apply under concurrent same-document mutation is now
        // BEHAVIORAL: `partition::convergence_concurrent_same_doc_merge_storm`
        // storms one PCounter doc from a 3-node mesh and asserts the exact sum
        // (below => a delta dropped, above => double-applied). Mixed-field legs
        // assert the product state (LWW name + counter views) after live, restart,
        // and 3-node full-mesh replay; `MC_MixedFieldMaterialization` proves the
        // stale whole-document commit hazard RED/GREEN.
        // The process-local document queue and counter-finalize gate are gone.
        // Ordinary application markers validate exactly-once decisions; counter
        // operands fold in the engine. Materialized mirrors retain validated
        // reads. The adapter test `interactive_counter_increment_conflicts_with_concurrent_same_doc_merge`
        // checks conflict/rollback/retry convergence. Engine OCC is a boundary;
        // the historical serialized MergeQueue model is deliberately retired.
        family: "Transaction & merge-queue concurrency",
        name: "INV_NoLoss / INV_NoDoubleApply under concurrent same-doc mutation",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/field_ops.rs apply_field_op_once; crates/database/db/src/write/doc.rs; crates/database/db/tests/merge/merge_handler_tests.rs; crates/database/db txn registry",
        model_ref: "MC_TxnRegistry_Green.cfg / MC_TwoStoreCounter_Green.cfg / MC_MixedFieldMaterialization_Green.cfg",
        tiers: &[Behavioral, Boundary],
        bound_by: "proofs/tests/behavioral/partition.rs convergence_concurrent_same_doc_merge_storm, convergence_concurrent_pncounter_same_doc_merge_storm, convergence_concurrent_mixed_lww_and_counter_fields_merge",
    },
    Property {
        // A counter block that arrives on its own before its document has a
        // value on the block's schema version is a terminal skip: no applied
        // key, no value, no merged mark. The composite that writes the value
        // applies it once. The db merge tests
        // `counter_merge_of_an_unmaterialized_document_waits_for_its_composite`
        // and `counter_merge_marks_cid_merged` check both arrival orders.
        family: "Transaction & merge-queue concurrency",
        name: "INV_NoLoss: a standalone counter block that arrives before its document's value is applied once, by its composite",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/field_ops.rs apply_field_op_once; crates/database/db/src/merge/merge_handler/batch.rs process_block_in_txn; crates/database/db/src/merge/merge_handler/composite_fields.rs; crates/database/db/tests/merge/merge_handler_tests.rs",
        model_ref: "MC_TwoStoreCounter_{Green,Red_LostDeferral}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/merge/merge_handler_tests.rs counter_merge_of_an_unmaterialized_document_waits_for_its_composite, counter_merge_marks_cid_merged",
    },
    Property {
        // A commit begins when it is handed to its transaction's owner, or,
        // if a request is running the transaction then, when the owner takes
        // it; a caller that stops waiting before that abandons it. One that
        // began still lands after its caller stops waiting. The caller raises
        // the database's fence count
        // and swaps the commit's state word; the owner swaps it once it is
        // finished, and whichever swap loses lowers the count. Every new
        // transaction and read waits in DB::new_txn while the count is above
        // zero, so it sees the commit.
        family: "Transaction & merge-queue concurrency",
        name: "INV_ReaderSeesCancelledCommit / INV_NeverNegative / INV_UncancelledUntouched / INV_SettledExact / INV_BegunEndRuns / INV_AbandonedEndDiscarded / LIVE_NoFenceLeak / LIVE_ReaderProceeds: a cancelled commit that began is fenced until it lands; one that had not begun is abandoned",
        axis: Tla,
        anchor: "crates/database/db/src/txn/fence.rs CommitFences, Fence::abandon, Fence::finish; crates/database/db/src/txn/owner/mod.rs Owners::run, Owners::finish, FenceOnDrop; crates/database/db/src/txn/owner/actor.rs own; crates/database/db/src/database/mod.rs DB::new_txn",
        model_ref: "MC_CommitFence_{Green,Red_RaiseAfterSwap,Red_OwnerNoSwap,Red_ReaderNoWait,Red_ForgetFailed,Red_UndoNoWake,Red_SkipBegun,Red_RunAbandoned}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A live HTTP server bounds its own wait to answer a transaction
        // commit. Past the bound, and only then, it answers "unknown"; the
        // worker-local commit keeps its ticket, so the answer never says or
        // causes an abort. Without the bound, a queued or running commit can
        // leave a live client waiting forever.
        family: "Transaction & merge-queue concurrency",
        name: "ClientSettles / UnknownIsNotAbort / AnswerOnlyAfterSend: the server settles a wedged commit answer by its own bound without cancelling it",
        axis: Tla,
        anchor: "crates/surfaces/http/src/server.rs Server::router commit bound, admitted handler task; crates/database/db/src/txn/owner/mod.rs Owners::finish, Owners::finish_detached; crates/database/db/src/txn/owner/actor.rs own",
        model_ref: "MC_CommitResponseWait_{Green,Red_NoBound,Red_Cancel}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/http/tests/commit_bound.rs a_commit_past_its_bound_answers_unknown_and_keeps_running, the_bound_is_only_for_transaction_commits, a_begin_is_not_bounded_as_a_commit; tools/integration-test/tests/testo/txn/http_commit_bound.rs an_http_commit_past_its_bound_answers_unknown_and_lands",
    },
    Property {
        family: "Transaction & merge-queue concurrency",
        name: "bound_answers_unknown / unknown_keeps_ticket / no_bound_waits: a passed bound answers unknown and keeps the commit ticket",
        axis: Lean,
        anchor: "crates/surfaces/http/src/server.rs Server::router commit bound",
        model_ref: "CommitResponseWait (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A request reaches an explicit transaction when its owner key is the
        // one the transaction began with. An HTTP registry keys a request by
        // its identity; the embedded node's registry gives every request one
        // key, since the embedding process is its only caller (#2335). Either
        // way the work runs as the request's own identity.
        family: "Transaction & merge-queue concurrency",
        name: "INV_FfiReaches / INV_HttpOwnerOnly / INV_ActsAsCaller: the embedder reaches its transactions as any identity, an HTTP caller only its own, and work runs as the request",
        axis: Tla,
        anchor: "crates/database/db/src/txn/registry/mod.rs TxnOwnership, DbTransactionRegistry::caller, DbTransactionRegistry::in_txn; crates/database/db/src/txn/registry/lifecycle.rs run_as; crates/database/db/src/txn/owner/table.rs Table::admits; crates/entry-points/ffi/src/node/mod.rs build_node_state; crates/entry-points/defra-node/src/node_builder.rs NodeBuilder::with_txn_ownership",
        model_ref: "MC_TxnAdmission_{Green,Red_IdentityOnFfi,Red_EmbedderOnHttp,Red_ActsAsOwner}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transaction & merge-queue concurrency",
        name: "INV_Agreement / INV_Foreign / INV_StartToken / INV_FinalAnswer / INV_OwnerSettled / LIVE_QuestionAnswered: a lost commit answer is queryable only by its owner, under its start token, on every poll, until the idle age, and a full store never drops a live record",
        axis: Tla,
        anchor: "crates/database/db/src/txn/owner/mod.rs Owners::outcome, Owners::outcome_status, OutcomeStore, OutcomeStore::insert, OutcomeStore::evict, Outcome::of, Outcome::is_final; crates/database/db/src/txn/registry/lifecycle.rs DbTransactionRegistry::outcome_as",
        model_ref: "MC_TxnOutcomeQuery_{Green,Red_Unknown,Red_Start,Red_Owner,Red_Consume,Red_PollOnce,Red_EvictLive}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transaction & merge-queue concurrency",
        name: "embedder_admits_every / byIdentity_admits_iff / byIdentity_turns_away_stranger / admits_beginner / default_is_byIdentity",
        axis: Lean,
        anchor: "crates/database/db/src/txn/registry/mod.rs TxnOwnership, DbTransactionRegistry::caller; crates/database/db/src/txn/owner/table.rs Table::admits",
        model_ref: "TxnAdmission (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transaction & merge-queue concurrency",
        name: "exactly_one_wins / one_lowering_per_raise / caller_win_means_owner_lowers / uncancelled_owner_wins: the two swaps settle each fence once",
        axis: Lean,
        anchor: "crates/database/db/src/txn/fence.rs Fence::abandon, Fence::finish",
        model_ref: "CommitFence.Swap (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
