use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "acp, zanzibar, vera, query, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NoStale / INV_SameHash / INV_AcceptValid: certified assignments honor the caller's position floor",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/provider/assignments.rs check_position",
        model_ref: "MC_AssignmentPosition_{Green,Red_Stale,Red_Hash}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera/src/vera_rs/provider/assignments.rs position_floor_rejects_replays_and_accepts_a_verified_new_term",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "AssignmentPosition.accepted_iff / stale_term_rejected / stale_height_rejected / same_height_hash_rejected / later_term_accepted / later_height_accepted",
        axis: Lean,
        anchor: "crates/engine/vera/src/vera_rs/provider/assignments.rs check_position",
        model_ref: "AssignmentPosition (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera/src/vera_rs/provider/assignments.rs position_floor_rejects_replays_and_accepts_a_verified_new_term",
    },
    Property {
        // When a takeover's cut may lie below the journal's floor, the journal
        // cannot vouch for what was voided. The provider closes it as
        // recoverable and rebuilds from the node's own ledger of confirmed
        // registrations and relationships, in bounded pages. A page is handed
        // to the outbox before its cursor is written, the pass is complete
        // only at the ledger's end, a cursor of an older term is not resumed,
        // and the provider admits nothing until the source says the pass for
        // the certified term is over, nor after a second outbox asked.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NothingLost / INV_TheirsNotLost / INV_RevocationDenies / LIVE_RecoveryEnds: an uncovered takeover recovers without a new worker, page by page through the outbox; no decision is served before everything the deployment lost is owed, nor once a second node's outbox asked, a committed revocation denies throughout, and the pass ends",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/provider/terms.rs Follower::check_with, Follower::recover; crates/engine/vera/src/vera_rs/provider.rs VeraRsProvider::owe_voided_to; crates/engine/vera/src/vera_rs/follow/journal.rs Journal::close_recoverable, Journal::recovery_needed, Journal::recovered; crates/engine/vera/src/vera_rs/follow/mod.rs Follow::admit, reconcile; crates/engine/acp/src/recovery.rs RecoveryLedger::page, RecoveryCursor, RecoverySource; crates/engine/acp/src/external/recovery.rs RecoverySource::next, RecoverySource::delivered; crates/engine/acp/src/external/voided.rs OweAgain::owe_again, OweAgain::settle_owed",
        model_ref: "MC_FollowDeploymentRecovery_{Green,Green_OneRecordPages,Red_NoGate,Red_NoHandOver,Red_CursorFirst,Red_CompleteEveryPage,Red_NeverDone,Red_NoTermFence,Red_NoSharedFence}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "Unified access observations preserve mode-specific gates and transactional authority",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/owners.rs; crates/database/db/src/read/lensed/fetcher/mod.rs",
        model_ref: "MC_UnifiedAccess_Green.cfg / MC_UnifiedAccess_Red_*.cfg",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/runner/owners_tests.rs unified_access_rejects_or_reports_unavailable_without_bound_acp, delegated_turn_uses_certified_acp_even_if_fetcher_fallback_is_unbound",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "Acp.OwnerObservation.accessDecision preserves delegated and named access gates",
        axis: Lean,
        anchor: "crates/engine/query/src/runner/owners.rs",
        model_ref: "Acp.OwnerObservation",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/runner/owners_tests.rs unified_access_rejects_or_reports_unavailable_without_bound_acp; crates/database/db/tests/read/owner_observation.rs owner_observation_rejects_a_claim_without_a_bound_transaction_view",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "Owner and visibility observations retain read gates and the transaction snapshot",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/owners.rs; crates/database/db/src/read/lensed/fetcher/mod.rs",
        model_ref: "MC_OwnerObservation_Green.cfg / MC_OwnerObservation_Red_*.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/read/owner_observation.rs owner_observation_rejects_a_claim_without_a_bound_transaction_view; crates/engine/query/src/runner/owners_tests.rs owner_observation_rejects_a_fetcher_without_a_transaction_view",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NoStaleOwner / INV_NoUnconfirmedAnswer / INV_NothingJoinsCommit / INV_OwedNamesCreator / INV_OwedRecordHeld: an owner lookup inside a transaction on a remote provider names an owed registration's creator from the transaction's view and commits on it only while that record is unchanged; any other owner is the provider's, read at the edge and kept; the commit looks for kept answers in the step that commits, and with answers kept no request joins it and every kept answer is confirmed again at a revision no older than the commit's start, an unreachable provider refusing the commit; a change finalized between that confirmation and the local commit is not caught (WindowWitness)",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/owners.rs remote_owner, owner_job; crates/engine/query/src/txn/observed_owners.rs ObservedOwners; crates/engine/query/src/runner/owner_confirmation.rs commit_confirming; crates/database/db/src/txn/registry/owned.rs commit_unless_told, commit_confirmed; crates/database/db/src/txn/owner/mod.rs finish_unless; crates/database/db/src/txn/owner/table.rs runs, send_request; crates/engine/query/src/txn/context.rs AcpRegistrationProjection",
        model_ref: "MC_RemoteOwnerObservation_{Green,Red_NoConfirm,Red_SplitLook,Red_NoSeal,Red_SlipUnconfirmed,Red_StaleAdmission,Red_OwedIgnored,Red_SettleUnchecked,Red_UnreachableCommits,Red_WindowWitness}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/remote_owner_observation.rs an_owner_lookup_in_a_transaction_answers_registered_created_and_owed_documents, an_owner_that_changed_before_the_commit_conflicts_and_commits_nothing, an_unreachable_provider_refuses_the_commit_and_keeps_the_transaction, a_settle_of_the_owed_registration_conflicts_a_decision_made_on_its_creator, a_transaction_that_asked_nobody_commits_without_a_round, a_request_made_during_the_confirmation_round_finds_no_transaction, a_commit_dropped_during_the_round_leaves_the_transaction_to_commit_again, a_commit_dropped_during_the_round_leaves_the_transaction_to_discard, a_guard_dropped_during_the_round_discards_its_transaction; crates/database/db/tests/txn/guard.rs cancellation_after_durable_commit_cannot_undo_it_or_discard_another_transaction; crates/database/db/src/txn/owner/table.rs a_request_reaches_only_an_entry_that_takes_requests; crates/entry-points/defra-node/tests/embedded_vera/owner_in_txn.rs an_owner_lookup_in_a_transaction_answers_on_an_embedded_vera_node, the_is_owner_route_answers_in_a_transaction_on_an_embedded_vera_node",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "RemoteOwnerAnswer.owed_names_creator, provider_only_when_not_owed, commit_only_if_confirmed, changed_answer_conflicts, unconfirmed_conflicts and split_answers_conflict",
        axis: Lean,
        anchor: "crates/engine/query/src/txn/observed_owners.rs ObservedOwners; crates/engine/query/src/runner/owners.rs remote_owner",
        model_ref: "RemoteOwnerAnswer",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/txn/observed_owners.rs nothing_recorded_commits, commit_only_if_every_answer_is_confirmed_as_itself, split_answers_to_one_request_never_commit",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NeverRevealed / INV_StrictComplete / INV_DenialNeverErrors / INV_LateWaited / INV_NamesNothing / INV_TxnUsable / INV_FailedReturnsNothing / INV_DefaultNeverFails: no read returns a row its caller may not read; a strict read answers with every row the caller may read or fails, returning no row, only on a check that had no answer after the provider's waits, never on a denial, naming no document and leaving its transaction usable; a default read never fails on a check and may answer partially (DefaultPartialWitness)",
        axis: Tla,
        anchor: "crates/engine/query/src/plan/permission_filter.rs AcpReadCheck; crates/engine/query/src/runner/strict_access.rs access_check_failure; crates/foundation/defra-core/src/request_context.rs strict_access_reads; crates/engine/query/src/error.rs AccessCheckUnavailable, from_acp; crates/engine/vera/src/vera_rs/provider/verified.rs late; crates/engine/vera/src/vera_rs/provider/reach.rs read_error",
        model_ref: "MC_StrictAccessRead_{Green,Red_Partial,Red_DenialErrors,Red_NoWait,Red_Reveal,Red_NamesRow,Red_TxnBroken,Red_PublishEarly,Red_DefaultPartialWitness}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/engine/query/src/plan/permission_filter.rs a_strict_read_fails_on_an_unanswered_check_and_only_filters_a_denial, permission_errors_fail_closed_without_stopping_the_scan; crates/entry-points/defra-node/tests/unit/strict_access_reads.rs a_row_whose_check_cannot_be_answered_is_dropped_without_an_error, a_strict_read_fails_where_the_default_drops_and_only_filters_a_denial, a_failed_strict_read_in_a_transaction_leaves_it_usable, a_strict_request_whose_prepared_access_input_cannot_be_answered_fails_classified, a_strict_top_level_aggregate_fails_classified; crates/entry-points/defra-node/tests/embedded_vera/strict_reads.rs a_strict_read_fails_while_the_deployment_cannot_be_reached; crates/engine/vera/src/vera_rs/provider/verified/tests.rs a_late_read_is_told_from_a_wrong_one",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "StrictAccessRule.denial_drops, allow_keeps, default_never_fails, strict_never_drops_unanswered, strict_fails_only_unanswered, retryable_iff_late and kept_only_allowed",
        axis: Lean,
        anchor: "crates/engine/query/src/plan/permission_filter.rs AcpReadCheck",
        model_ref: "StrictAccessRule",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/plan/permission_filter.rs a_strict_read_fails_on_an_unanswered_check_and_only_filters_a_denial",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "Acp.OwnerObservation.exact_snapshot, unreadable_observation_hidden and unregistered_read_keeps_visibility",
        axis: Lean,
        anchor: "crates/engine/query/src/runner/owners.rs",
        model_ref: "Acp.OwnerObservation",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/read/owner_observation.rs owner_observation_rejects_a_claim_without_a_bound_transaction_view; crates/engine/query/src/runner/owners_tests.rs owner_observation_rejects_a_fetcher_without_a_transaction_view",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_RevocationConsistent + both User and _commits paths gated",
        axis: Tla,
        anchor: "crates/engine/acp; crates/building-blocks/zanzibar; crates/engine/query dag-scan",
        model_ref: "MC_Acp_Green.cfg / MC_Commits_Green.cfg",
        tiers: &[Behavioral, Contract],
        bound_by: "proofs/tests/behavioral/acp.rs acp_dual_path_gating_and_revocation; proofs/tests/lean_conformance.rs lean_relation_expression_vocab_matches_rust",
    },
    Property {
        // The light client checks an answer at the newest root it has
        // authenticated, and refuses one that a block finalized during the
        // round trip overtook. The vera provider asks such a read again, at
        // most ATTEMPTS times, and a read the node answered busy (-32002,
        // evidence not ready by its deadline) again after a jittered pause,
        // only while that pause ends within BUSY_PATIENCE of its first ask,
        // the requests' own time included; the two are counted apart. An
        // error reaches a caller only after every overtaken ask, or once no
        // pause fits in the patience, and a stale answer is never accepted.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NeverStale / INV_FailsOnlyWhenEveryAskOvertaken / INV_AttemptsBounded / INV_NeverFromStale / INV_StaleFailsOnlyAtPatience / LIVE_Answers: a read at a stale head waits within its patience for the next revision and is never decided there; a proven read the chain overtook or the node answered busy is asked again, bounded by a count and by elapsed time, never accepted stale",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/provider/verified.rs verified, verified_within, Pace, WallPace, JITTER, ATTEMPTS, BUSY_PATIENCE, BUSY_PAUSE, busy, stale; crates/building-blocks/inputs/src/retry.rs Jitter, pause; crates/engine/vera/src/vera_rs/provider.rs create_policy, query_policy, query_object_owner, verify_access, create_access_decision; backbone:acp-light-client HeaderChain::accept_response, HeaderChain::voided",
        model_ref: "MC_VerifiedRead_{Green,Green_Exhausted,Green_Stale,Red_AcceptStale,Red_NoRetry,Red_Uncapped,Red_NoBusyRetry,Red_BusyUncapped,Red_PausesOnly,Red_UndrawnPause,Red_NoStaleWait,Red_DecideStale}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A document deleted on a node whose provider archives on delete
        // (vera.rs) owes the archive of its object in the delete's
        // transaction. Whoever the policy lets delete may delete; the archive
        // is made as the owner and sent only under the owner's authority,
        // parked until the owner hands one in when the node holds none, and
        // a delete by someone the policy does not let delete is refused
        // before commit; earlier unsent changes on the object go with the
        // delete; an owed archive denies everyone on the object, leaving the
        // document's commit history only to an owner that deleted it itself,
        // and decides
        // before a registration still owed to the creator or the
        // unregistered rule; a change already in flight lands before the
        // archive, and Vera refuses any change to an archived object.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_DeletedGrantsNothing / INV_NoGrantAfterArchive / INV_DeletedByPermitted / INV_ArchiveAsOwner / INV_HistoryOnlyToDeleter / INV_DeleteChargesNobody / LIVE_Retired: a deleted document grants nothing, no grant outlives its archive, only whom the policy lets delete deletes it, its archive is owed and sent only as its owner, only its deleter reads its commit history, its archive counts against no actor's admission of new changes, nor the node's, nor what its transaction may stage, and its object ends archived",
        axis: Tla,
        anchor: "crates/engine/acp/src/outbox/staging.rs TransactionOutbox::stage_archive; crates/engine/acp/src/outbox/changes.rs OwedChanges::drop_unsent_on, OwedChanges::stage_archive; crates/engine/acp/src/outbox/deny.rs owed_denial, archive_actor; crates/engine/acp/src/outbox/ops.rs account, live_on; crates/engine/acp/src/outbox/changes.rs OwedChanges::stage; crates/engine/acp/src/outbox/submission.rs relationship_call; crates/database/db/src/write/autocommit/helpers.rs archive_deleted_doc; crates/engine/query/src/txn/context.rs decided_by_owed, spared_by_owed_archive, decided_by_owed_archive; crates/engine/query/src/txn/read_access.rs OverlayChecker::object_access; crates/engine/acp/src/external.rs ExternalDocumentAcp::check_doc_access",
        model_ref: "MC_ArchiveOnDelete_{Green,Green_NoDrop,Red_GrantAfterArchive,Red_DeleteWithoutPermission,Red_ArchiveAsDeleter,Red_ArchiveChargesOwner,Red_ArchiveChargesNode,Red_ArchiveChargesTxn,Red_HistoryForTheOwner,Red_NoOverlay,Red_HistoryForAll,Red_OwedRegistrationFirst,Red_UnregisteredFirst}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/access/external_archive.rs an_owed_archive_leaves_its_actor_the_commit_history_and_nothing_else, an_owed_archive_denies_before_an_owed_registration_or_an_unregistered_object_could_grant, a_permitted_non_owners_delete_commits_and_denies_the_document_to_everyone_at_once, a_non_owners_delete_parks_the_archive_until_the_owners_authority_appears; crates/engine/acp/src/external/archive_tests.rs a_non_owners_deletes_leave_the_owners_admission_untouched, a_node_full_of_parked_archives_still_admits_new_changes, a_transactions_deletes_leave_its_own_staging_bound_untouched",
    },
    Property {
        // An account's Vera deployment runs on every device, one active at a
        // time; a takeover without a handover voids the blocks above its cut,
        // and the light client's height goes down. The provider decides and
        // sends nothing while a certified term is unchecked, caches decisions
        // under the term it last checked, sends again exactly as signed the
        // journaled submissions the deployment no longer holds, owes again
        // through the outbox what a replay does not restore (and everything
        // after it on the same object), and fails closed for good when the
        // term may have voided more than the journal keeps. Where a replay
        // starts is written down before it begins, so one cut short is run
        // again from there; what it leaves to owe is published with the
        // checked term and blocks admission until the outbox holds it. The
        // embedder is also told once what each replay came to.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NoVoidDecision / INV_RevocationDenies / INV_NothingLost / LIVE_ResubmittedSettles: no decision is served from a voided revision, a revocation confirmed on a void block still denies, nothing a term voided is taken as kept, and what is owed again settles",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/follow/mod.rs Follow::admit, Follow::still, Follow::epoch, reconcile, scan, uncovered; crates/engine/vera/src/vera_rs/follow/journal.rs Journal::record, Journal::begin_replay, Journal::handled, MAX_CONFIRMATIONS, MAX_CONFIRMATION_BYTES; crates/engine/vera/src/vera_rs/follow/lost.rs lost_effects, tainted_by; crates/engine/vera/src/vera_rs/follow/report.rs TakeoverReport, Applied, Unrestorable; crates/engine/vera/src/vera_rs/provider/terms.rs Follower::check_with, Follower::publish; crates/engine/vera/src/vera_rs/provider.rs decision_state, admitted_read; crates/engine/vera/src/vera_rs/provider/owed_request.rs locked_worker; crates/engine/acp/src/external/voided.rs OweAgain, VoidedEffect; backbone:acp-light-client HeaderChain::voided",
        model_ref: "MC_FollowDeployment_{Green,Green_Window,Red_KeepsCache,Red_ForgetsRevocation,Red_NoGate,Red_NoTaint,Red_AssumesWindow,Red_NoMark,Red_NoDeliveryGate,Red_CollapsesBatch}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A takeover report is not an internal count: the embedder is told
        // once per checked term, by the call that checks it, which
        // submissions were applied again, which are owed again, and which
        // cannot be restored, with the cut of that term. A check that failed
        // closed says so. A call that only delivers reports nothing more.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_Classified / INV_Published / INV_PublishedOnce / INV_Complete / INV_CutOfTerm / INV_ClosedNamed / LIVE_Reported: every replayed submission is classified as applied again, owed again or unrestorable, an unrestorable outcome carries its reason, a checked term is reported once, after every replay and with the owed ones, under the cut of its own term, and a check that failed closed says so",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/follow/report.rs TakeoverReport, Applied, Unrestorable; crates/engine/vera/src/vera_rs/follow/mod.rs reconcile, first_check; crates/engine/vera/src/vera_rs/follow/lost.rs lost_effects; crates/engine/vera/src/vera_rs/provider/terms.rs Follower::check_with, Follower::certificate, Follower::owed_effects, Follower::publish, takeover_report; crates/engine/vera/src/vera_rs/follow/journal.rs Journal::close; crates/foundation/events/src/event.rs AcpTakeoverReconciledData",
        model_ref: "MC_TakeoverReport_{Green,Red_NoPublish,Red_NoReason,Red_Early,Red_Duplicate,Red_DropsOwed,Red_NewestCut,Red_UnnamedClosed,Red_UnrecordedClose}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The vera provider drives a journaled submission to its receipt:
        // ask, send when no send is known to have arrived, ask again, under
        // one deadline. A send that timed out may have been admitted, so it
        // is sent again; the node spends a sequence once, so the request
        // takes effect at most once. A call ends early only on a failure
        // that cannot pass. The journal keeps the request until a receipt is
        // acknowledged, so a later call confirms what an earlier one could
        // not, also after the node dropped an admitted copy.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_AtMostOneEffect / INV_ConfirmedHasEffect / INV_NoEarlyGiveUp / INV_RetainedUntilConfirmed / LIVE_UndisturbedCallConfirms / LIVE_RecalledConfirms: a pending submission is resent until confirmed, takes effect once, is given up only at its deadline or on a failure that cannot pass, stays journaled until confirmed, and is confirmed by a later call when an earlier one was cut short",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/provider/submission.rs resolve_pending, recover_pending, send_tx_with_sequence; crates/engine/vera/src/vera_rs/receipt_wait.rs until_receipt; crates/engine/vera/src/vera_rs/client.rs retryable; crates/engine/vera/src/vera_rs/worker.rs acknowledge; crates/engine/vera-embedded/src/submission.rs",
        model_ref: "MC_PendingSubmission_{Green,Green_Recalled,Red_NoResend,Red_NoDedupe,Red_GivesUpEarly,Red_ForgetsOnTimeout,Red_Recalled_Forgets}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Registrations of one document that race each write their own owner
        // key, so only the commit can refuse all but one. The deciding prefix
        // scan is a validated scan: a registration that commits under the
        // prefix after another's snapshot makes the other conflict, and it
        // returns false. The sentinel byte it replaces never conflicted on
        // regolith, which commits a write that leaves a key as it was.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_OneOwner / INV_WinnersOwn / INV_SomeoneWins: racing registrations of one document leave exactly one owner",
        axis: Tla,
        anchor: "crates/engine/acp/src/persistent.rs PersistentAcpStore::register_doc_atomic, delete_doc_tuples; crates/engine/acp/src/store.rs AcpStore::register_doc_atomic",
        model_ref: "MC_RegisterDocOnce_{Green,Red_NoValidation,Red_Sentinel}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A delete keeps the document's ACP object, so the history the node
        // retains reads under the same decision as before; a purge removes
        // the object with the history (a truncate keeps it, as Go does), and
        // a deleted DocID cannot be added again until then.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NoUnintendedRead: nobody reads a document's history whom its current owner has not let read it, across delete, revocation, purge and re-creation",
        axis: Tla,
        anchor: "crates/database/db/src/write/autocommit/helpers.rs archive_deleted_doc, register_created_doc; crates/database/db/src/write/autocommit/mod.rs register_deleted_doc_with_acp_as; crates/engine/query/src/runner/mutation.rs execute_single_mutation; crates/database/db/src/merge/merge_handler/composite_persist.rs handle_deletion; crates/database/db/src/collection/acp.rs release_purged_doc_objects; crates/engine/acp/src/read_access.rs DirectChecker",
        model_ref: "MC_DeletedHistory_{Green,Red_UnregisterOnDelete,Red_KeepOnPurge,Red_NoFence}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/acp/dac/deleted_history.rs a_deleted_documents_history_stays_hidden_from_an_actor_without_access, a_delete_committed_in_a_transaction_keeps_the_history_hidden, a_reader_keeps_a_deleted_documents_history_until_revoked, a_purged_document_added_again_takes_no_earlier_grant",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "DeletedHistory.delete_keeps_access / deleted_history_hidden_from_foreign / revoke_after_delete_denies_reader / unregister_on_delete_leaks / recreate_refused_while_deleted / purged_then_recreated_has_no_reader",
        axis: Lean,
        anchor: "crates/database/db/src/write/autocommit/helpers.rs archive_deleted_doc, register_created_doc; crates/database/db/src/collection/acp.rs release_purged_doc_objects; crates/engine/acp/src/read_access.rs DirectChecker",
        model_ref: "DeletedHistory (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/acp/dac/deleted_history.rs a_deleted_documents_history_stays_hidden_from_an_actor_without_access, a_reader_keeps_a_deleted_documents_history_until_revoked, a_purged_document_added_again_takes_no_earlier_grant",
    },
];
