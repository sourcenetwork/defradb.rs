use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "acp, db, query, defra-node, storage, vera (owed relationship changes specified before the code, #2954; owed counts in staged and settled parts, #3070; the drainer's in-flight mark, journal and batches, #3065)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The registrations one transaction's creates owe an external
        // provider are settled together: an object the provider holds is
        // cleared unsent, one creator's go as one ordered list applied all or
        // none, and a refused list is sent again one registration at a time,
        // first to last, so each is answered as alone.
        family: "ACP outbox and owed registrations",
        name: "INV_ClearedOnlyHeld / INV_OwnerIsCreator / INV_RefusedOnlyIfBad / INV_NoResendOfKnownHeld / INV_HeldNotFailed / INV_Order: a record is cleared only once its object is held (for its creator, or for another node's owner it yields to), a list carries one creator, only a registration refused alone is parked, no request carries an object the settler found held, a refusal of an object another settler registered meanwhile is cleared as held, and a refused list's members go alone in order",
        axis: Tla,
        anchor: "crates/engine/acp/src/commit_registrations.rs CommitRegistrations::owe, settle, grouped; crates/engine/acp/src/external/registration_batch.rs settle_all_if_owed, to_send, send_group, Sender; crates/engine/acp/src/external/registration.rs held_after_refusal, registered; crates/engine/acp/src/external/replica.rs yield_replica_if_held_elsewhere; crates/engine/vera/src/dac/registration_batch.rs register_listed, register_one_by_one; crates/engine/vera/src/vera_rs/provider/registration_list.rs calldata; crates/database/db/src/txn/mod.rs DbTxn::owe_acp_registration, OwedAcpRegistration::run; crates/database/db/src/collection/acp.rs collection_to_register",
        model_ref: "MC_RegistrationBatch_{Green,Green_Lost,Green_Elsewhere,Red_NoRecheck,Red_NoAskOnRefusal,Red_NoFallback,Red_MixedCreators,Red_OutOfOrder}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/access/registration_batch.rs the_creates_of_one_transaction_are_registered_in_one_submission, a_refused_member_is_answered_as_alone_and_the_others_are_registered, a_list_whose_answer_was_lost_is_not_sent_again, each_creator_of_one_transaction_gets_a_list_of_its_own, a_document_another_settler_registered_meanwhile_is_cleared_as_held, a_registration_another_settler_made_meanwhile_is_cleared_as_held, a_document_held_for_another_owner_is_cleared_and_left_out_of_the_list; crates/entry-points/defra-node/tests/embedded_vera/registration_batch.rs ten_creates_in_one_transaction_are_registered_in_one_block",
    },
    Property {
        // The request a creator's registrations go to vera.rs in: refused
        // before sending when empty, longer than vera.rs's list or over the
        // byte bound; otherwise one command per object, in their order.
        family: "ACP outbox and owed registrations",
        name: "request_count / request_order / request_bytes / request_built / commands_objects: a registration list holds 1 to 64 commands within the byte bound, names its objects in their order, and a list within every bound whose policies read is built",
        axis: Lean,
        anchor: "crates/engine/vera/src/vera_rs/provider/registration_list.rs calldata; crates/engine/vera/src/provider.rs MAX_REGISTRATIONS_PER_REQUEST",
        model_ref: "VeraRegistrationList (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "ACP outbox and owed registrations",
        name: "INV_NoOverWideDenial / INV_RevokedAuthorityDenies: staging after an owed revocation",
        axis: Tla,
        anchor: "crates/engine/acp/src/outbox/staging.rs advise, stage_prepared, authority_revoked; crates/building-blocks/zanzibar/src/types/policy.rs management_sources",
        model_ref: "MC_OwedStagingAuthority_{Green,Red_Wide,Red_Self,Red_NoOwner}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/access/external_overlay.rs the_owner_changes_an_object_after_revoking_a_tuple_on_it, a_manager_changes_an_object_after_revoking_a_tuple_its_authority_does_not_rest_on, a_manager_whose_managing_tuple_is_revoked_changes_nothing_more; crates/building-blocks/zanzibar/src/types/policy_tests.rs a_direct_manager_is_its_own_only_source, a_computed_manager_reads_the_relation_it_is_computed_from, a_tuple_to_userset_reads_its_tuple_relation_on_this_object_only, unions_intersections_and_differences_read_every_part, a_cycle_ends_with_each_relation_once",
    },
    Property {
        family: "ACP outbox and owed registrations",
        name: "restsOnRevoked / other_relation_spares / staged_by_self_still_takes / unknown_sources_any_wide_takes",
        axis: Lean,
        anchor: "crates/engine/acp/src/outbox/staging.rs rests_on_revoked",
        model_ref: "OwedStagingAuthority (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/acp/src/outbox/staging.rs only_a_revoked_source_naming_the_actor_or_everyone_takes_its_authority",
    },
    Property {
        // A remote provider cannot join the row transaction, so a create
        // writes the registration it owes into that transaction and the
        // node settles it after the commit and again at start. Until the
        // record is cleared the gate lets only the creator read. A
        // relationship change made in a transaction is staged the same way
        // and submitted after the commit: its actor is checked at staging,
        // an add is staged unchecked when the provider cannot be asked and a
        // delete is not staged then, an owed delete denies from the commit
        // on, an owed add grants nothing, the registration settles first,
        // one tuple's changes go one behind the other, a refused change is
        // parked if it was checked and removed if it was not, a parked
        // change holds its tuple only if it was checked, and an unchecked
        // one that waits is removed when a later change of its tuple passes
        // it. All of that is anchored here. The node's drainer submits by
        // itself, after each commit that staged a change and after a
        // back-off while the provider is away. Through a provider whose
        // worker journals its requests (vera.rs) a change is marked in
        // flight with its request's digest, then its hash, before the
        // request goes; a marked change is decided by the journal, a
        // verified receipt or the note of whoever resolved the request,
        // and sent again only when none of them knows it (the Journal
        // configurations); the changes one transaction staged one after the
        // other share a request, and a refused one is sent again one change
        // at a time (INV_RefusedOnItsOwn, Red_BatchNotIsolated). Through a
        // provider without a journal the code matches the model with
        // Marker = FALSE (Green_NoMarker): a change the provider applied
        // before a crash is sent again (Red_NoInFlightMarker). The creator's
        // own named grant on a document still owed is ahead of code.
        // INV_AtMostOneEffect also rests on a receipt or note staying
        // readable (Red_ReceiptLost, Red_JournalReceiptLost) and on the note
        // (Red_JournalNoNote). A note found by digest counts only from the
        // mark's sequence floor on (Green_JournalSameBytes, Red_JournalNoFloor:
        // INV_SettledTookEffect), and a journaled request lands only under the
        // delegation it carries (Red_JournalRenewedCredential:
        // INV_LandsUnderItsCredential). A client's change staged over HTTP
        // acts as its verified credential's DID (Red_RemoteActorFromRequest:
        // INV_ActsAsItsCredential) and is checked at staging
        // (Red_RemoteUnchecked: INV_DenialsChecked), and is staged only under
        // an authority the node can send it with, its own key for that DID or
        // the delegation presented (Green_Remote_HeldKey, Red_RemoteNoAuthority:
        // INV_SendableAuthority). The creator-only gate rests on the provider not
        // already holding the document for another owner
        // (Red_ContestedRegistration).
        family: "ACP outbox and owed registrations",
        name: "INV_NoWidening / INV_RevokedAtCommit / INV_RegistrationFirst / INV_AtMostOneEffect / INV_RollbackLeavesNothing / INV_OwedDocumentNarrow / INV_DenialsChecked / INV_RefusedChecked / INV_UncheckedYields / INV_PresentedAuthority / INV_FailureReported / INV_AbandonIsFinal / INV_RefusedOnItsOwn / INV_SettledTookEffect / INV_LandsUnderItsCredential / INV_ActsAsItsCredential / INV_SendableAuthority / LIVE_Drains: what a node owes a remote ACP provider never widens access before it is settled, a committed delete denies at once and until the provider holds it or its own actor abandons it, the registration settles before any change and only on its creator's word (its record, a delegation recorded with a change the creator owes, or the bearer a request carries for the creator), a registration whose submission failed says so and a change deleted without the provider holding it leaves a note until its actor was told, a change takes effect once across a crash while its receipt, journal entry or note can be read and never after it was abandoned, a change is parked as refused only for its own refusal, a rollback leaves nothing owed, no delete denies on an unchecked word, no unchecked change is parked as refused or holds a later change of its tuple, a client's change staged over HTTP acts as its credential's DID and carries an authority the node can send it with, and every committed change ends settled, parked or held behind a parked change of its tuple",
        axis: Tla,
        anchor: "crates/engine/acp/src/pending.rs PendingRegistration, PendingRegistrations::record, PendingRegistrations::get, PendingRegistrations::clear; crates/engine/acp/src/external/registration.rs settle, settle_with, registrar, recorded_registrar, presenting, note_failure, rewrite, clear; crates/foundation/defra-core/src/request_context.rs RequestContext::bearer_token_for; crates/engine/acp/src/external.rs ExternalDocumentAcp::redrive_owed, ExternalDocumentAcp::check_doc_access, ExternalDocumentAcp::redrive_registration, ExternalDocumentAcp::revoked, ExternalDocumentAcp::transaction_outbox; crates/engine/acp/src/external/settle.rs settle_owed_changes_once, pass, pass_unchecked, settle_change, conclude, submittable, register_first, creators_delegation, settle_owed_before, refusal; crates/engine/acp/src/external/drain.rs recover_marked, send_request, batch, joins_request, settle_applied, MAX_CHANGES_PER_REQUEST; crates/engine/acp/src/outbox/journal.rs OwedJournal, PreparedRequest, RequestOutcome; crates/engine/acp/src/outbox/changes.rs OwedChanges::stage, VALIDATED_WITHIN_OF_NODE_LIMIT; crates/entry-points/defra-node/src/owed_drainer.rs drain, Backoff, PublishRefusals; crates/engine/vera/src/vera_rs/provider/owed_request.rs prepare_owed_request, journal_owed, owed_outcome; crates/engine/vera/src/vera_rs/provider/submission.rs resolve_pending, recover_pending, Note; crates/engine/vera/src/vera_rs/outcome_note.rs OutcomeNote, MAX_NOTED; crates/engine/vera/src/dac/owed_journal.rs VeraOwedJournal; crates/engine/acp/src/outbox/staging.rs TransactionOutbox::stage, TransactionOutbox::advise, unchecked; crates/engine/acp/src/external/owed.rs entries, await_transaction, reauthorize, abandon_change; crates/engine/acp/src/outbox/ops.rs stage, settle, mark, mark_in_flight, clear_in_flight, unsettled_validated, reauthorize, discard, acknowledge; crates/engine/acp/src/outbox/record.rs run_epoch, InFlight; crates/engine/acp/src/outbox/deny.rs owed_denial, OwedDenials::on; crates/engine/acp/src/outbox/submission.rs Submission; crates/database/db/src/write/autocommit/helpers.rs register_created_doc, archive_deleted_doc; crates/database/db/src/write/autocommit/create.rs run_create_attempt; crates/database/db/src/txn/mod.rs DbTxn::owe_acp_registration; crates/engine/acp/src/commit_registrations.rs CommitRegistrations::owe; crates/database/db/src/txn/mod.rs DbTxn::owed_staged, rollback_statement_savepoint; crates/engine/query/src/txn/context.rs AcpRegistrationProjection, decided_by_owed; crates/entry-points/defra-node/src/acp_ops/transactional.rs relationship_in_txn, settle_owed_changes; crates/entry-points/defra-node/src/node_acp.rs gate_external_acp; crates/entry-points/defra-node/src/node_core/owed.rs NodeCore::owed_acp, NodeCore::await_owed_acp, NodeCore::reauthorize_owed_acp, NodeCore::abandon_owed_acp_change; crates/engine/vera/src/dac/owed.rs submit; crates/surfaces/http/src/handlers/acp.rs relationship_operations, add_doc_relationship, remove_doc_relationship; crates/entry-points/defra-node/src/ops/http_ops.rs HttpNodeOps::change_doc_relationship, HttpNodeOps::supports_transactional_relationships; crates/engine/vera/src/dac.rs authority",
        model_ref: "MC_AcpOutbox_{Green,Green_Registered,Green_Three,Green_Three_Registered,Green_Live,Green_NoMarker,Green_OwnerChanges,Green_Live_OwnerChanges,Green_Journal,Green_Journal_Registered,Green_Journal_Live,Green_JournalSameBytes,Green_Remote,Green_Remote_HeldKey,Green_Remote_Live,Red_OwedOutsideTxn,Red_NoDenyOverlay,Red_LatestOpOverlay,Red_RefusedDeleteForgotten,Red_OwedAddGrants,Red_WildcardGrant,Red_ChangeBeforeRegistration,Red_OutOfOrder,Red_DrainPastAwaiting,Red_NoInFlightMarker,Red_ReceiptLost,Red_ContestedRegistration,Red_RefusedBlocksDrain,Red_UncheckedDelete,Red_UncheckedRefusedKept,Red_UncheckedHolds,Red_UncheckedPassedKept,Red_TransferWhileOwed,Red_AbandonByAnyone,Red_AbandonInFlight,Red_SilentFailure,Red_SilentDiscard,Red_AmbientRegistration,Red_AmbientRecorded,Red_ReauthorizeUnchecked,Red_JournalNoMarker,Red_JournalNoNote,Red_JournalReceiptLost,Red_BatchNotIsolated,Red_JournalNoFloor,Red_JournalRenewedCredential,Red_RemoteActorFromRequest,Red_RemoteUnchecked,Red_RemoteNoAuthority}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/access/external_owed.rs a_registration_that_fails_after_its_commit_is_reported_with_its_reason, a_registration_with_no_authority_waits_for_a_credential_and_is_not_sent, changes_behind_a_parked_registration_go_once_it_is_given_a_credential, a_wait_says_pending_while_the_provider_is_away_and_ends_when_a_record_changes, only_the_actor_that_staged_a_refused_delete_lifts_its_denial_by_abandoning_it, an_abandoned_change_lets_the_changes_behind_it_go_and_only_a_refused_one_is_abandoned, a_refused_change_is_submitted_again_under_a_fresh_credential, a_change_the_provider_never_took_is_reported_as_discarded_and_not_as_settled, a_wait_is_for_a_transaction_of_this_run_and_for_its_callers_entries; crates/entry-points/defra-node/tests/unit/owed_report.rs only_the_actor_that_staged_a_refused_revocation_abandons_it, a_retry_of_an_owed_registration_never_reads_the_token_the_process_holds, a_request_that_carries_no_bearer_registers_nothing_under_a_token_the_process_holds, a_pass_sends_a_registration_with_whichever_owed_delegation_the_report_found; crates/database/db/tests/access/external_drainer.rs a_change_committed_before_a_crash_is_sent_once_after_the_restart, a_change_marked_before_a_crash_is_decided_by_the_note_and_not_sent_again, a_change_marked_before_a_crash_is_decided_by_the_journal_and_not_sent_again, a_change_whose_receipt_was_kept_before_a_crash_is_settled_by_its_hash, an_unavailable_provider_leaves_the_change_marked_until_it_comes_back, a_refused_change_is_parked_and_told_of_and_holds_back_no_other_tuple, the_changes_one_transaction_staged_go_in_one_request, a_refused_request_is_sent_again_one_change_at_a_time, a_change_whose_request_nobody_knows_is_tried_again_without_a_commit, a_marked_change_is_sent_by_a_provider_without_a_journal, transactions_on_a_document_commit_while_the_drainer_settles_there; crates/engine/vera/src/vera_rs/outcome_note.rs an_outcome_of_an_earlier_request_with_the_same_bytes_is_not_the_callers, a_recorded_hash_reads_its_own_outcome_and_no_other; tools/integration-test/tests/vera/owed_note.rs a_command_that_resolves_a_journaled_request_with_its_bytes_notes_the_outcome; crates/database/db/tests/access/external_counts.rs near_the_node_bound_stagers_are_ordered_and_settles_abort_none; crates/entry-points/defra-node/tests/unit/http_staged_relationship.rs a_client_transaction_creates_and_grants_and_the_grant_lands_after_the_commit, a_change_without_authority_is_refused_before_the_commit_and_owes_nothing, a_discarded_or_abandoned_transaction_leaves_nothing_owed, the_acting_identity_comes_from_the_credential_alone, an_identity_token_for_a_did_whose_key_the_node_holds_stages_under_the_node_key; crates/entry-points/defra-node/tests/embedded_vera/http_staged.rs a_client_transaction_that_creates_and_grants_commits_and_the_grantee_reads, a_refused_or_discarded_grant_owes_nothing; crates/entry-points/defra-node/tests/embedded_vera.rs an_idle_node_settles_a_staged_grant_once_its_deployment_is_back, the_grants_one_transaction_stages_land_together, a_restart_settles_a_change_the_chain_could_not_take",
    },
    Property {
        // What the deny overlay and the node's bound decide on is counted
        // per object (deletes wider than one actor), per actor a delete
        // names on an object, and for the node, each as one value in a
        // staged part that only staging merges into and a settled part that
        // only settling merges into. The deny decision and the bound read
        // the staged part alone, validated, and stagers of one tuple are
        // ordered by its head key, which settling never touches; so a settle
        // aborts none of them, while a delete committed after a decision
        // refuses it. A fully settled count is never deleted: it stays one
        // folded operand and reads as nothing owed.
        family: "ACP outbox and owed registrations",
        name: "INV_NoMissedRevoke / INV_SettleAbortsNobody / INV_OwedNotNegative: a settle never aborts a stager or a deny decision, a decision never commits past a delete it did not see, and nothing more is settled than was staged",
        axis: Tla,
        anchor: "crates/engine/acp/src/outbox/ops.rs owed_at, staged_parts, stage, settle, account, unsettled_validated, tuple_state; crates/engine/acp/src/outbox/deny.rs owed_denial, denial; crates/engine/acp/src/outbox/changes.rs OwedChanges::stage, VALIDATED_WITHIN_OF_NODE_LIMIT; crates/engine/acp/src/outbox/keys.rs head_key, object_key, subject_key, live_key, object_live_prefix, LEGACY_QUEUE_PREFIX; crates/engine/storage/src/backends/regolith/merge/owed.rs touches, fold, full",
        model_ref: "MC_OwedParts_{Green,Red_SettleTouchesStaged,Red_UnvalidatedDecision,Red_UnguardedSettle}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/access/external_counts.rs staging_another_tuple_of_a_document_while_one_settles_commits, staging_the_same_tuple_while_its_earlier_change_settles_commits, staging_on_another_document_while_one_settles_commits, two_transactions_staging_one_tuple_still_conflict, a_decision_commits_past_a_settle_on_its_document, a_revocation_committed_after_a_decision_refuses_it, a_grant_committed_after_a_decision_does_not_refuse_it, every_kind_of_subject_is_denied_as_its_delete_says, what_is_fully_settled_or_rolled_back_owes_nothing, near_the_node_bound_stagers_are_ordered_and_settles_abort_none",
    },
    Property {
        // The owed-state operand: four counters, each one part, added
        // counter by counter. Folding is exact, associative and commutative;
        // a fold touches what either operand touches; settling touches no
        // staged part; under the write rule nothing settled passes what was
        // staged; and a reader of the staged part with an older settled part
        // can only find more owed.
        family: "ACP outbox and owed registrations",
        name: "apply_fold / fold_assoc / fold_comm / fold_exact / touches_fold / settle_touches_no_staged_part / stage_touches_staged / settled_le_staged / run_sound / late_settled_over_owes / late_settled_never_misses / fully_settled_owes_nothing: the owed-state counters fold exactly, settling and staging touch disjoint parts, what is owed is never negative, and a late settlement only denies more",
        axis: Lean,
        anchor: "crates/engine/storage/src/backends/regolith/merge/owed.rs delta, read, full, fold, touches, part",
        model_ref: "OwedParts (lake build)",
        tiers: &[Contract],
        bound_by: "crates/engine/storage/src/backends/regolith/merge/tests/owed.rs owed_folds_obey_the_engine_contract, owed_touches_obey_the_engine_contract, staging_and_settling_touch_disjoint_parts, a_read_of_the_staged_part_meets_only_staging, a_fully_settled_value_owes_nothing_before_and_after_compaction",
    },
    Property {
        // The live sequences of one owed tuple: one key each, the tuple's
        // prefix and the sequence as eight big-endian bytes. Byte order of
        // the keys is numeric order and each reads back as its sequence, so
        // a prefix scan lists them ascending; the first is the oldest owed
        // and the last the latest, before and after any one settles, and an
        // empty index names neither.
        family: "ACP outbox and owed registrations",
        name: "decode_encode / order_iff / lex_iff_val / empty_index / oldest_is_min / latest_is_max / settle_index / oldest_after_settle / latest_after_settle / ten_and_twenty: live-sequence keys sort and read back as their sequences, and the first and last key read are the oldest and latest change still owed after any settle",
        axis: Lean,
        anchor: "crates/engine/acp/src/outbox/keys.rs live_prefix, live_key, live_seq; crates/engine/acp/src/outbox/ops.rs tuple_state, stage, settle",
        model_ref: "OwedLive (lake build)",
        tiers: &[Contract],
        bound_by: "crates/engine/acp/src/outbox/keys.rs the_live_keys_of_a_tuple_sort_by_sequence_and_name_only_it; crates/database/db/tests/access/external_outbox.rs settling_a_change_leaves_the_later_ones_of_its_tuple_owed, settling_out_of_order_keeps_the_oldest_the_latest_and_the_deletes_true",
    },
    Property {
        // A document's owner gives it to another actor through the outbox:
        // the transfer is owed in the caller's transaction, sent only once
        // the registration has settled and every earlier change on its
        // object is decided, and every later change on the object waits for
        // it. It is not atomic with the create: from its commit the old
        // owner is denied on the object, before an owed registration could
        // let it in, and the new owner gets nothing until the provider names
        // it. A refusal is kept and told, a repeat refused because the new
        // owner is already named is settled as applied, and an answer that
        // never decides is refused after a bounded number of tries. Of a
        // transfer and another change staged on one object at once, the one
        // that commits second is aborted, so the queue follows commit order.
        family: "ACP outbox and owed registrations",
        name: "INV_NoWidening / INV_EarlierUnderOwner / INV_LaterAfterTransfer / INV_QueueFollowsCommits / INV_RegistrationFirst / INV_TransferOnce / INV_RefusedTookNoEffect / INV_FailureReported / LIVE_Decided: an owed transfer widens nothing for its old or its new owner, the creator's changes committed before the transfer land while it still owns the object and those committed after it after the transfer, the queue orders changes around a transfer as their transactions committed, nothing goes before the registration, the transfer takes effect once and is never reported refused after it took effect, a removal is told, and every committed change is decided",
        axis: Tla,
        anchor: "crates/engine/acp/src/outbox/staging/transfer.rs TransactionOutbox::stage_transfer; crates/engine/acp/src/outbox/ops.rs account, stage; crates/engine/acp/src/outbox/keys.rs staged_key, transfer_key; crates/engine/acp/src/outbox/deny.rs owed_denial; crates/engine/acp/src/outbox/staging.rs advise; crates/engine/query/src/txn/context.rs decided_by_owed_archive; crates/engine/acp/src/external.rs ExternalDocumentAcp::check_doc_access, denied_outright; crates/engine/acp/src/external/transfer_order.rs held_by_transfer, earlier_on_object; crates/engine/acp/src/external/settle.rs submittable, register_first, conclude, confirmed_transfer, MAX_FAILED_SUBMISSIONS; crates/engine/acp/src/external/drain.rs joins_request, send_request, recover_marked; crates/entry-points/defra-node/src/acp_ops/transfer.rs transfer_in_txn",
        model_ref: "MC_OwnerTransfer_{Green,Green_Live,Green_Poisoned,Green_NoMarker,Red_TransferFirst,Red_CommitOrder,Red_CommitOrderReverse,Red_LaterFirst,Red_CreatorReadsUntilApplied,Red_NewOwnerEarly,Red_BeforeRegistration,Red_RefusedAfterEffect,Red_SilentDrop,Red_RetryForever}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/engine/acp/src/external/transfer_tests.rs a_transfer_waits_for_its_objects_earlier_changes_and_later_ones_wait_for_it, an_owed_transfer_denies_its_old_owner_and_grants_its_new_owner_nothing, a_refused_transfer_is_kept_and_told_and_denies_until_its_actor_abandons_it, a_transfer_whose_answer_was_lost_is_settled_and_not_reported_refused, a_transfer_staged_beside_a_grant_that_commits_first_is_aborted, a_settle_beside_a_staged_transfer_does_not_abort_it, a_grant_staged_beside_a_transfer_that_commits_first_is_aborted, a_managers_grant_staged_beside_a_transfer_that_commits_first_is_aborted, concurrent_grants_on_one_object_do_not_conflict; crates/entry-points/defra-node/tests/unit/transfer_owner.rs a_transfer_lands_once_after_the_creators_grant_and_hands_the_document_over, an_owed_registration_holds_the_transfer_which_lands_once_after_a_restart, a_refused_transfer_is_reported_and_leaves_the_document_its_creators; crates/entry-points/defra-node/tests/embedded_vera/transfer.rs a_transferred_document_is_its_new_owners_on_the_chain_after_its_creators_grant, a_transferred_gated_document_keeps_its_derived_relationship",
    },
    Property {
        // The order around a transfer and the denial it puts on its old
        // owner, as pure decisions.
        family: "ACP outbox and owed registrations",
        name: "heldByTransfer / abortedBy / around_a_transfer_one_of_two_commits / ordinary_changes_do_not_conflict / a_transfer_waits_for_any_earlier_change / a_change_waits_for_an_earlier_transfer / only_a_transfer_holds_an_ordinary_change / not_held_means_ordered / a_transfer_denies_its_old_owner: a transfer waits for every earlier live change on its object outside its request, a later change for an earlier live transfer and nothing else, a refused change holds nothing, an owed transfer denies its old owner outright, and of a transfer and another change staged on one object at once the second to commit is aborted while two ordinary changes never abort each other",
        axis: Lean,
        anchor: "crates/engine/acp/src/external/transfer_order.rs held_by_transfer; crates/engine/acp/src/outbox/deny.rs denial; crates/engine/acp/src/outbox/ops.rs account, stage",
        model_ref: "OwnerTransferOrder (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/engine/acp/src/external/transfer_order.rs a_transfer_waits_for_every_earlier_live_change_and_a_change_for_an_earlier_transfer, a_refused_change_and_a_change_of_the_same_request_hold_nothing",
    },
];
