//! The conformance registry: every modeled family's headline property, the
//! model that proves it, the source it is anchored to, and how it is bound to
//! the real binary.
//!
//! This table is the spine: its distinct `family` values are the modeled
//! families. `matrix::readme_lists_exactly_the_modeled_families` asserts that
//! `proofs/README.md`'s *Modeled families* table lists exactly these, so a new
//! model cannot land without declaring how it is kept honest against the code.

/// Which proof tool establishes the property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// TLA+ / TLC temporal-safety model (`proofs/tla/`).
    Tla,
    /// Lean 4 functional/algebraic proof (`proofs/lean/`).
    Lean,
}

/// How a property is checked for conformance with the implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Exercised against the running release binary (`tests/tla_conformance.rs`).
    Behavioral,
    /// A model vocabulary/contract asserted against the live Rust types
    /// (`tests/lean_conformance.rs`) — anti-drift, no binary needed.
    Contract,
    /// An assumed boundary (crypto / connectivity / bounded-N / foreign
    /// substrate): surfaced, never asserted, so a green matrix never reads as
    /// "this was proven against the artifact."
    Boundary,
}

pub struct Property {
    /// Family name — must match a row in `proofs/README.md`'s Modeled families table.
    pub family: &'static str,
    /// The headline invariant or theorem.
    pub name: &'static str,
    pub axis: Axis,
    /// The source the model is grounded to: paths and the symbols in them,
    /// resolved against the tree by `anchor::every_registry_anchor_resolves`.
    pub anchor: &'static str,
    /// The TLC config (`*.cfg`) or Lean theorem that establishes it.
    pub model_ref: &'static str,
    /// How this property is bound to the implementation.
    pub tiers: &'static [Tier],
    /// The tests that bind a Contract or Behavioral property, in anchor
    /// syntax: each file and the test functions in it. Empty for a property
    /// that is only a Boundary.
    pub bound_by: &'static str,
}

use Axis::{Lean, Tla};
use Tier::{Behavioral, Boundary, Contract};

pub const PROPERTIES: &[Property] = &[
    Property {
        family: "Transactional node source grants",
        name: "HostNoopProjection rechecks descendants with the original actor",
        axis: Tla,
        anchor: "crates/engine/query/src/txn/source_nac.rs; crates/database/db/src/document_projection.rs",
        model_ref: "MC_HostNoopProjection_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transactional node source grants",
        name: "HostSourceNoop actual changes require authority and observations validate",
        axis: Tla,
        anchor: "crates/engine/query/src/txn/source_nac.rs; crates/database/db/src/nac/trait_impl.rs",
        model_ref: "MC_HostSourceNoop_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transactional node source grants",
        name: "ProjectorMutationOrigin actual actor and atomic publication",
        axis: Tla,
        anchor: "crates/database/db/src/document_projection.rs; crates/database/db/src/nac/trait_impl.rs",
        model_ref: "MC_ProjectorMutationOrigin_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // `execute_as` signs a node's writes through the shared signed-query
        // context while keeping its caller's identity and DAC bypass; a
        // missing signer fails closed. Boundary, bound by the focused node
        // signing test.
        family: "Node identity and signing",
        name: "NodeSigningChoice.execute_as_keeps_the_caller / execute_as_signs_as_the_caller / execute_as_unsigned_without_signing / execute_as_refuses_a_missing_signer",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::execute_as; crates/entry-points/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/foundation/defra-core/src/request_context.rs RequestContext::with_signing_session",
        model_ref: "NodeSigningChoice (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/execute_as_signing.rs execute_as_signs_writes_and_keeps_the_caller_identity",
    },
    Property {
        // A node's identity and its signing are separate choices, as in Go:
        // an identity-only node runs each request as its caller and never
        // signs, and a node built without `with_identity_only` runs requests
        // exactly as before.
        family: "Node identity and signing",
        name: "NodeSigningChoice.default_build_unchanged / identity_only_runs_as_caller / identity_only_never_signs / identity_only_never_refused",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::execute_prepared_request; crates/entry-points/defra-node/src/assemble.rs assemble; crates/entry-points/defra-node/src/node_builder.rs NodeBuilder::with_identity_only",
        model_ref: "NodeSigningChoice (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A transaction the node begins and every request the node runs in it
        // agree on its owner: the node's principal, which is no one on an
        // identity-only node. The tests run that agreement on the live node.
        family: "Node identity and signing",
        name: "NodeSigningChoice.begin_and_execute_agree / owner_before_disagreed / default_owner_unchanged",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::node_principal, Node::node_context, Node::execute_prepared_request; crates/entry-points/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/entry-points/defra-node/src/ops/request.rs txn_owner",
        model_ref: "NodeSigningChoice (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/embedded_signer.rs an_identity_only_node_runs_requests_in_the_transactions_it_begins; crates/entry-points/defra-node/tests/nac_embedded_test.rs an_identity_only_node_is_checked_as_its_caller_under_nac",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LocalFetchPermit.released_before_send / release_preserves_queue",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag",
        model_ref: "LocalFetchPermit (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/dag_fetcher_tests.rs local_complete_fetch_releases_peer_permit_before_waiting_to_send",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "NoPermitWhileSending / OnePermit — locally complete fetch releases its peer permit before queueing merge",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag; crates/database/p2p/src/sync/coordinator/dag_fetcher_tests.rs local_complete_fetch_releases_peer_permit_before_waiting_to_send",
        model_ref: "MC_LocalFetchPermit_{Green,Red_Hold}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/dag_fetcher_tests.rs local_complete_fetch_releases_peer_permit_before_waiting_to_send",
    },
    Property {
        family: "Pull sync (doc-sync / sync-branchable)",
        name: "Delivered / OneReplyEach / RefusedOnlyUnsent / NoResendAfterDeadline — a pubsub_rpc request is held until a peer subscribes, resent to each later subscriber, and never resent after its deadline",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/held_request.rs HeldRequest::next, HeldRequest::publish, HeldRequest::publish_by_deadline, HeldRequest::finish; crates/database/p2p/src/sync/coordinator/pubsub_services.rs PubsubServices::peer_subscribed",
        model_ref: "MC_HeldPubsubRequest_{Green,Red_NoHold,Red_NoResend,Red_LateWatch,Red_NoDedupe,Red_ResendLate}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/held_request_tests.rs doc_sync_sent_before_the_topic_has_a_peer_waits_for_one",
    },
    Property {
        family: "Transactional node source grants",
        name: "INV_Authority / INV_Private / INV_NoRevival / INV_Independent / INV_Reconciled / INV_RequestUnchanged",
        axis: Tla,
        anchor: "crates/engine/acp/src/nac/node_acp/source.rs; crates/engine/query/src/txn/source_nac.rs; crates/database/db/src/document_projection.rs",
        model_ref: "MC_SourceScopedNac_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transactional node source grants",
        name: "remove_other / remove_self / direct_survives / owner_survives / admin_survives / denied / untrusted_noop_denied / embedded_unchanged_allowed / embedder_replay_reconciles / embedder_replay_never_widens / embedder_missing_unauthorized_refused / embedder_complete_replay_needs_no_authority / embedder_replay_ignores_node_authority / untrusted_no_replay",
        axis: Lean,
        anchor: "crates/engine/acp/src/nac/node_acp/source.rs; crates/engine/query/src/txn/source_nac.rs; crates/database/db/src/document_projection.rs; crates/entry-points/defra-node/tests/unit/source_nac.rs",
        model_ref: "SourceScopedNac (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/source_nac.rs source_admission_matches_executed_lean_model",
    },
    Property {
        family: "Held-bounded node permission delegation",
        name: "delegated_no_escalation / delegated_never_touches_admin / admin_behavior_preserved / admin_acceptances_kept / delegate_admin_refused / delegate_unheld_refused / delegate_admin_class_refused / delegate_wildcard_refused / delegate_privileged_delete_refused / delegation_not_transferable / admits_observe",
        axis: Lean,
        anchor: "crates/engine/acp/src/nac/delegation.rs DelegationFacts::admits, ADMIN_CLASS; crates/engine/acp/src/nac/node_acp/operations.rs NodeACP::authorize_change; crates/engine/acp/src/nac/node_acp/source.rs NodeACP::change_source_permission",
        model_ref: "NodeAcpDelegation (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/delegated_nac.rs delegation_matches_executed_lean_model, delegate_grants_and_revokes_only_held_non_admin_permissions, delegate_source_edges_follow_the_same_rule",
    },
    Property {
        family: "vera.rs actor spelling",
        name: "defra_idempotent / spellings_meet / other_unchanged / defra_same_actor / one_spelling_per_actor / wire_is_taken / wire_same_actor / defra_of_wire / wire_secp256k1_is_defra",
        axis: Lean,
        anchor: "crates/engine/vera-delegation/src/did_spelling.rs defra, wire, same_actor",
        model_ref: "VeraActorSpelling (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "vera.rs delegation signing",
        name: "admitted_iff_vera_accepts / sent_is_accepted / refused_is_not_sent / honest_answer_admitted / honest_ed25519_answer_admitted / ed25519_refusal_is_length_or_verification / refusal_names_the_first_failure / ed25519_refusal_names_the_first_failure",
        axis: Lean,
        anchor: "crates/engine/vera-delegation/src/signer/admission.rs admit, actor_key; crates/engine/vera-delegation/src/bearer.rs create_scoped_bearer_token_at",
        model_ref: "VeraDelegationSigner (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "vera.rs delegation window",
        name: "issued_is_well_formed / issued_is_what_veras_builder_writes / request_holds_iff / written_at_provider_time_holds / ahead_within_allowance_holds / ahead_beyond_allowance_refused / behind_by_the_lifetime_refused / longer_than_the_longest_lifetime_refused_ahead / plain_ahead_within_allowance_holds / without_allowance_one_second_ahead_refused / reported_lead_is_a_lower_bound / unset_start_is_reported_exactly / set_back_start_is_reported_less_the_allowance / expired_claims_nothing_about_the_clock / outlasting_reaches_too_far / rewritten_at_reported_time_holds",
        axis: Lean,
        anchor: "crates/engine/vera-delegation/src/bearer/request.rs issue_request_at, MAX_REQUEST_LIFETIME; crates/engine/vera-delegation/src/bearer.rs not_before; crates/engine/vera/src/vera_rs/bearer.rs authorize_at; crates/engine/vera/src/vera_rs/bearer/window.rs within_window; crates/engine/acp/src/delegation_window.rs CLOCK_SKEW_ALLOWANCE, WindowRefusal",
        model_ref: "VeraDelegationWindow (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "vera.rs embedded transport",
        name: "within_iff / network_accepted_is_accepted / longer_is_refused / answer_in_time_is_kept / silent_node_ends_the_wait / late_answer_is_not_waited_for / node_error_keeps_its_code / busy_node_is_asked_again / slow_node_is_asked_again / stopped_node_is_not_asked_again / asked_again_iff / asked_again_as_over_http / client_errors_stay_distinct / transport_errors_stay_distinct",
        axis: Lean,
        anchor: "crates/engine/vera-embedded/src/call.rs within; crates/engine/vera-embedded/src/deadline.rs bounded; crates/engine/vera-embedded/src/submission.rs client_error; crates/engine/vera-embedded/src/acp.rs transport_error; crates/engine/vera/src/vera_rs/client.rs ClientError::retryable",
        model_ref: "VeraEmbeddedTransport (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The embedded node streams only headers of blocks that finalize
        // after a subscription; a fresh deployment's first block finalized
        // before it, and a lone validator proposes the next only at its
        // heartbeat. The transport starts the stream with the newest
        // finalized block, read after the subscription stands, and the light
        // client checks it as it checks every header.
        family: "vera.rs embedded transport",
        name: "INV_OnlyFinalized / INV_Current / INV_SentHeadFresh: the light client takes only a header a finalized block certifies, once the stream is drained it stands on the newest fresh finalized block in its term without waiting for another, and the transport never sends a head too old to take",
        axis: Tla,
        anchor: "crates/engine/vera-embedded/src/acp.rs subscribe_headers, headers_after; crates/engine/vera-embedded/src/acp/head.rs finalized, header, recent; backbone:acp-light-client HeaderChain::start",
        model_ref: "MC_FirstHeader_{Green,Red_NoHead,Red_ReadBeforeSubscribe,Red_SendStale,Red_DropLaterTerms,Red_Unverified}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera-embedded/src/acp/tests.rs queued_headers_at_or_below_the_fetched_head_are_not_emitted, a_header_of_a_later_term_at_or_below_the_fetched_head_is_emitted; crates/engine/vera-embedded/tests/embedded_node.rs a_fresh_deployments_provider_starts_on_its_first_block; crates/entry-points/defra-node/tests/embedded_vera/first_start.rs a_fresh_deployments_first_start_takes_its_first_block",
    },
    Property {
        family: "vera.rs deployment terms",
        name: "missing_is_suffix / takeWhile_is_filter / scan_finds_missing / floor_covers_pruned: the submissions a takeover voided are exactly the journal's suffix from the first void one, found by asking newest first, and the newest pruned entry vouches for every older one",
        axis: Lean,
        anchor: "crates/engine/vera/src/vera_rs/follow/mod.rs reconcile, uncovered; crates/engine/vera/src/vera_rs/follow/journal.rs Journal::record, Journal::prune, Floor",
        model_ref: "VeraConfirmationJournal (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Explicit embedded signing",
        name: "admitted_binds",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/embedded_signer.rs admitted_signer; crates/entry-points/defra-node/src/lib.rs execute_prepared_request",
        model_ref: "EmbeddedSigner (lake build); embedded_signer::native_admission_matches_executed_lean_model",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/embedded_signer.rs native_admission_matches_executed_lean_model",
    },
    Property {
        family: "Explicit embedded signing",
        name: "INV_Bound",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/embedded_signer.rs admitted_signer; crates/entry-points/defra-node/src/lib.rs execute_prepared_request",
        model_ref: "MC_EmbeddedSigner_{Green,Red_*}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/embedded_signer.rs selected_actor_signs_create_update_and_respects_transaction_finality, selected_signer_does_not_bypass_document_acp",
    },
    Property {
        family: "Verified client graph transaction",
        name: "exact operation and graph, active bound authority, revocation and atomic mixed-actor commit",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/lib.rs execute_verified_graph_in_txn; crates/database/db/src/block/builder/verified_client_create.rs; crates/database/db/src/block/builder/verified_client_mutation.rs; crates/engine/query/src/runner/turn.rs lent",
        model_ref: "MC_VerifiedClientGraph_{Green,Red_*}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/verified_create.rs verified_user_create_shares_node_transaction_with_agent_effects, verified_update_and_delete_use_exact_actor_and_transaction_state, verified_create_allows_disabled_nac_and_unbound_collection",
    },
    Property {
        family: "Verified client graph transaction",
        name: "exact evidence, signed actor, consumed wave and transaction-bound authority",
        axis: Lean,
        anchor: "crates/database/db/src/block/builder/verified_client_create.rs; crates/database/db/src/block/builder/verified_client_mutation.rs; crates/entry-points/defra-node/src/lib.rs execute_verified_graph_in_txn",
        model_ref: "VerifiedCreateHandoff and VerifiedMutationHandoff (lake build); client_create_graph::signed_create_evidence_matches_the_modeled_exact_handoff",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/block/client_create_graph.rs signed_create_evidence_matches_the_modeled_exact_handoff, trusted_delete_graph_matches_model_and_refuses_other_waves; crates/entry-points/defra-node/tests/unit/verified_create.rs verified_update_and_delete_use_exact_actor_and_transaction_state",
    },
    Property {
        family: "Verified client graph transaction",
        name: "INV_AuthorityAtAdmission / INV_GrantAuthority / INV_RefusalRecordsNothing / INV_RequestOnlyItsRequest / INV_RequestIsNoLogin / INV_NothingSentStale / INV_RegistrationFirst / INV_SentUnderRecordedAuthority / INV_ReauthorizedWithAuthority / INV_FailureReported / INV_EditOnVerifiedPermission / INV_RequestBoundNoEdit / INV_EditOwesNothing: a verified write on a remote ACP provider is staged only with authority for its signer, a verified update or delete only when the signer held the permission at a verified revision no older than the freshness limit (owed registrations and revocations first), never with a request-bound delegation and owing nothing, a refusal records nothing, a delegation bound to one request is used for the command it names and never as a login, nothing is sent after expiry, a grant follows its document's registration, a command is sent only with the credential recorded for it, a parked command is owed again only on a fresh credential the rule of admission takes, and a submission that fails after the commit is reported",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/lib.rs execute_verified_graph_in_txn; crates/entry-points/defra-node/src/signer_credential.rs SignerCredential::carried_by; crates/engine/query/src/runner/remote_authority.rs remote_write_authority, edit_credential; crates/engine/query/src/runner/mutation.rs authorize_mutation; crates/engine/query/src/txn/context.rs check_doc_access_with_prepared_inputs; crates/engine/acp/src/external.rs ExternalDocumentAcp::check_doc_access, proves_decisions; crates/engine/vera/src/cosmos/dac.rs check_doc_access, proves_decisions; crates/engine/vera/src/vera_rs/provider.rs verify_access, proves_decisions, decision_state; crates/engine/vera/src/cosmos/dac.rs authority; crates/engine/vera/src/cosmos/dac/request_bound.rs request_bound, admit_for, with_policy_command; crates/engine/vera/src/vera_rs/bearer/request.rs caller_of_request_delegation, bound_caller; crates/engine/vera/src/vera_rs/bearer.rs caller_of_delegation; crates/engine/vera-delegation/src/command_digest.rs request_digest; crates/engine/vera/src/cosmos/dac/owed.rs token, lapsed; crates/engine/acp/src/outbox/staging.rs TransactionOutbox::stage; crates/engine/acp/src/external/settle.rs register_first; crates/engine/acp/src/pending.rs PendingRegistration; crates/engine/acp/src/external/registration.rs settle_with, registrar, note_failure; crates/engine/acp/src/external/owed.rs reauthorize, entries; crates/database/db/src/write/autocommit/helpers.rs register_created_doc; crates/entry-points/defra-node/src/owed.rs Node::reauthorize_owed_acp_as, Node::owed_acp_as",
        model_ref: "MC_VerifiedRemoteWrite_{Green,Red_NoAuthority,Red_AnyIssuer,Red_AnyNode,Red_ExpiredAccepted,Red_AnyCommand,Red_RequestLogsIn,Red_RecordedFirst,Red_SentExpired,Red_GrantFirst,Red_GrantUnchecked,Red_AmbientRetry,Red_ReauthUnchecked,Red_SilentFailure,Red_NoPermission,Red_StaleEvidence,Red_OwedIgnored,Red_RequestEdit,Red_EditOwes,Red_WindowWitness}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/verified_remote.rs a_verified_create_is_staged_only_with_authority_for_its_signer, a_request_bound_delegation_covers_the_registration_it_names_and_nothing_else, a_grant_in_a_verified_transaction_takes_its_own_request_bound_delegation, a_request_bound_delegation_that_expires_before_submission_parks_its_change; crates/entry-points/defra-node/tests/unit/verified_edit.rs a_verified_update_or_delete_is_admitted_on_the_signers_permission, every_credential_form_is_decided_by_the_permission, a_revocation_finalized_before_admission_refuses_the_update, an_update_fails_closed_without_fresh_evidence, what_the_node_owes_decides_before_the_provider; crates/entry-points/defra-node/tests/embedded_vera/verified.rs a_verified_create_under_the_signers_delegation_registers_the_signer, a_verified_create_under_a_request_bound_delegation_registers_the_signer, a_grant_staged_in_a_verified_transaction_lands_after_the_registration; crates/entry-points/defra-node/tests/embedded_vera/verified_edit.rs a_verified_update_and_delete_are_admitted_on_the_chains_permission, a_writer_whose_grant_was_revoked_on_chain_is_refused, a_verified_update_is_refused_when_the_chain_cannot_be_reached; crates/entry-points/defra-node/tests/unit/owed_report.rs a_registration_that_fails_after_its_commit_is_reported_and_takes_a_fresh_credential, a_change_waiting_for_authority_takes_a_credential_bound_to_it, a_submission_that_fails_for_an_unreachable_provider_is_reported_and_sent_by_the_wait, a_retry_of_an_owed_registration_never_reads_the_token_the_process_holds",
    },
    Property {
        family: "Verified client graph transaction",
        name: "INV_RegistrantIsReceiver / INV_OwnCreatorNotPreempted / INV_GenesisWithRegistration / INV_CreatorNotRejectedWhileOwed / INV_DeniedOnlyByProvider / INV_MergeAuthorized / INV_RevokedWriterDenied / INV_RegisteredStays / LIVE_RowMerges: a receiving node registers a foreign creator's replicated document as its provider's actor with the creator's grants as one owed unit written with the genesis, never pre-empts an own creator, rejects nobody on the word of an owed registration nor the creator while its grants are owed, merges a change only while its signer may write, and keeps a deleted document registered",
        axis: Tla,
        anchor: "crates/database/db/src/merge/replica.rs ReplicaRegistration, ReplicaOwnership, ReplicaStep; crates/database/db/src/merge/merge_handler/replica_plan.rs plan_replica_frame; crates/database/db/src/merge/merge_handler/composite_persist.rs record_replica; crates/database/db/src/merge/acp_merge_handler.rs AcpCompositeMergeHook::plan_replica, AcpCompositeMergeHook::on_protected_update, AcpCompositeMergeHook::node_reads; crates/engine/acp/src/replica.rs ReplicaMarker, ReplicaOwed; crates/engine/acp/src/external/replica.rs record_replica_unit, record_adopted_replica, owed_on_replica, yield_replica_if_held_elsewhere, settle_replica_unit; crates/engine/acp/src/external/registration.rs register_and_clear; crates/database/db/src/write/autocommit/helpers.rs unregister_deleted_doc",
        model_ref: "MC_ReplicaRegistration_{Green,Green_Live,Red_RegisterOwn,Red_RegisterAfterMerge,Red_RejectWhileOwed,Red_GrantsSeparate,Red_AdoptDrops,Red_NoAdoptOnArrival,Red_OwnedDenies,Red_StaleDecision,Red_ArchiveOnDelete,Red_DeleteClears}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/merge/replica_registration.rs a_foreign_creators_row_merges_once_the_receiver_registers_it_and_its_changes_follow, the_creators_update_while_the_unit_is_owed_is_put_back_and_then_merges, another_signers_change_waits_while_the_receivers_registration_is_owed, a_same_deployment_creators_row_is_never_registered_by_the_receiver, an_unlisted_collection_waits_for_a_registration_as_before, an_update_reaching_the_receiver_before_its_genesis_registers_through_the_genesis, a_revoked_writers_later_update_is_parked, a_second_receiver_waits_for_the_owners_grants_and_never_refuses_the_creator, a_receiver_that_sees_the_genesis_after_another_registered_it_waits_for_the_owners_grant, a_local_provider_registers_the_genesis_signer_as_before; crates/database/db/tests/access/external_owed.rs a_document_deleted_before_its_registration_settles_is_still_registered",
    },
    Property {
        family: "Verified client graph transaction",
        name: "INV_OnlyGranted / INV_SettledFirst / INV_RevokedStops / INV_RegisteredFirst / INV_ValidBatchCommits / INV_OrdinaryAdmitted / INV_CountBalanced / LIVE_AddressedMerges: on a remote provider the reader a created document's genesis asks for is owed by its creator from the create's own transaction behind the document's registration and counted with it, so no batch its registrations allow is refused and it takes no ordinary change's place in an admission count, and a node of the deployment merges the document only once its provider holds both, only if it is named, and nothing more of it once it has seen the reader revoked",
        axis: Tla,
        anchor: "crates/database/db/src/write/genesis_reader.rs stage_genesis_reader; crates/engine/acp/src/outbox/staging.rs TransactionOutbox::stage_with_registration; crates/engine/acp/src/outbox/changes.rs OwedChanges::stage_with_registration; crates/engine/acp/src/outbox/ops.rs account; crates/database/db/src/write/autocommit/helpers.rs register_created_doc; crates/engine/acp/src/external/settle.rs register_first; crates/database/db/src/merge/acp_merge_handler.rs AcpCompositeMergeHook::on_protected_composite, AcpCompositeMergeHook::node_reads",
        model_ref: "MC_RemoteGenesisReader_{Green,Green_Live,Red_Address,Red_Ungated,Red_Staged,Red_Memo,Red_Unregistered,Red_Separate,Red_Order,Red_Counted,Red_Admitted,Red_Unbalanced}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/access/external_genesis_reader.rs an_addressed_create_owes_its_reader_and_the_provider_takes_it_after_the_registration, a_public_create_owes_everyone_as_its_reader, a_mutable_address_or_a_collection_not_opted_in_owes_no_reader, a_batch_past_the_transactions_change_bound_owes_every_reader_and_takes_another_grant, a_batch_past_the_actors_unsettled_bound_owes_every_reader, owed_readers_leave_their_actor_room_to_revoke, owed_readers_take_nothing_of_the_nodes_bound_and_settle_without_a_trace; crates/entry-points/defra-node/tests/embedded_vera_replication/addressed.rs a_note_addressed_to_a_node_of_the_home_reaches_that_node_only",
    },
    Property {
        family: "Verified client graph transaction",
        name: "INV_NeverQuarantined / INV_MergeAuthorized / INV_HistoryFirst / INV_Bounded / LIVE_RedrivenOnGrant / LIVE_OtherBranchMerges: a replicated change whose signer a remote provider refuses is parked by its document, never rejected for good, re-driven with a fresh decision when this node changes the document's relationships or by the bounded sweep, never merged before its history, within the index bound, and a change on another branch is not held behind it",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/composite.rs prepare_composite_merge; crates/database/db/src/merge/merge_handler/protected_update.rs Refusal, check_protected_update; crates/database/db/src/merge/governance/awaited.rs WaitKey::AcpObject; crates/database/db/src/merge/governance/deferred.rs DeferredMerges::park, DeferredMerges::release_parked, MAX_WAITERS_PER_DEPENDENCY, MAX_DEFERRED_COMPOSITES; crates/database/db/src/merge/merge_handler/parked_acp.rs ParkedRelease, watch_parked_acp, sweep_parked_acp; crates/database/db/src/merge/acp_merge_handler.rs AcpCompositeMergeHook::parks_refusals; crates/engine/acp/src/dac.rs ObjectChangeListener; crates/engine/acp/src/external.rs ExternalDocumentAcp::object_changed",
        model_ref: "MC_ParkedMerge_{Green,Green_Live,Red_Quarantine,Red_MergeOverParked,Red_RedriveAdmits,Red_Unbounded,Red_NoRelease,Red_ParkBlocksObject}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/merge/parked_refusals.rs a_revoked_writers_change_is_parked_and_merges_once_the_node_grants_it_again, a_change_built_on_a_parked_one_waits_with_it_and_one_on_another_branch_merges, the_sweep_re_drives_a_change_a_grant_made_elsewhere_lets_in, an_unsigned_change_is_still_rejected; crates/database/db/src/merge/governance/deferred.rs the_sweep_releases_parked_composites_only",
    },
    Property {
        // The provider gate after the conversion to parking: the hook's
        // judgement (on_protected_update, parks_refusals) is executed over
        // every combination of its inputs against the Rust hook; the one
        // match in check_protected_update that turns a signed refusal into a
        // parked one is bound by the merge tests below.
        family: "Verified client graph transaction",
        name: "hook_never_parks / signed_never_rejected_under_remote / signed_refusal_parks_under_remote / unsigned_refusal_is_rejected / unsigned_is_rejected / parked_only_from_signed_refusal / allowed_merges / merged_only_when_allowed / rejected_only_on_refusal / owed_registration_rejects_nobody / creator_waits_for_its_grants / creator_waits_for_the_owners_grants / nothing_owed_the_provider_decides / other_signers_ignore_creator_grants / local_provider_unchanged / lax_access_unchanged / local_rejected_only_on_refusal / local_never_parks / local_reads_nothing_owed: under strict access with a remote provider a signed refusal is parked and only an unsigned one is rejected; a local provider or lax access decides as before",
        axis: Lean,
        anchor: "crates/database/db/src/merge/acp_merge_handler.rs AcpCompositeMergeHook::on_protected_update, AcpCompositeMergeHook::parks_refusals, AcpCompositeMergeHook::replicas_for; crates/database/db/src/merge/merge_handler/protected_update.rs Refusal, check_protected_update, frame_signer; crates/database/db/src/merge/merge_handler/hook.rs CompositeFrame; crates/engine/acp/src/replica.rs ReplicaOwed",
        model_ref: "ReplicaGate (lake build; lake env lean --run ReplicaGate.lean)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/src/merge/gate_conformance_tests.rs the_acp_hook_matches_the_executed_gate_model; crates/database/db/src/merge/merge_handler/protected_update.rs a_signer_that_is_not_a_did_is_judged_unsigned; crates/database/db/tests/merge/parked_refusals.rs a_revoked_writers_change_is_parked_and_merges_once_the_node_grants_it_again, an_unsigned_change_is_still_rejected",
    },
    Property {
        // The index the parked refusals are filed in, a data structure with
        // its invariants proven over any sequence of operations and its
        // observable results executed against the Rust type.
        family: "Verified client graph transaction",
        name: "inv_run / bounded_after_any_run / no_dangling_after_any_run / present_iff / has_waiters_iff / release_takes / release_leaves_no_trace / release_keeps / release_keeps_waiting / release_hands_back / release_idempotent / wf_run / releaseParked_count / picked_are_documents / release_parked_hands_back_only_parked / release_parked_keeps_governance / release_parked_drains / release_parked_twice_finds_nothing / defer_when_full_changes_nothing / defer_full_key_unchanged / defer_keeps_others / defer_keeps_other_waiters / park_of_room / park_of_full / park_files_under_both / park_past_document_bound / parked_released_by_either: the deferred-composite index keeps its per-key and per-node bounds and has no dangling entry after any operations, filing never drops another composite, a parked composite is filed under its document and the refused composite and leaves both when either releases it, a parked composite its document has no room for is not indexed at all, a release is idempotent, and the sweep hands back only composites parked under a document",
        axis: Lean,
        anchor: "crates/database/db/src/merge/governance/deferred.rs DeferredMerges::defer, DeferredMerges::park, DeferredMerges::release, DeferredMerges::release_parked, DeferredMerges::enqueue_ready, DeferredMerges::take_ready, DeferredMerges::forget, DeferredMerges::take_entry, DeferredMerges::unindex, MAX_DEFERRED_COMPOSITES, MAX_WAITERS_PER_DEPENDENCY, MAX_AWAITED_PER_COMPOSITE; crates/database/db/src/merge/governance/awaited.rs WaitKey::AcpObject, Deferral; crates/database/db/src/merge/merge_handler/composite.rs prepare_composite_merge",
        model_ref: "DeferredMerges (lake build; lake env lean --run DeferredMerges/Conformance.lean)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/src/merge/governance/deferred_conformance_tests.rs the_index_matches_the_executed_model; crates/database/db/src/merge/governance/deferred.rs the_sweep_releases_parked_composites_only, a_change_parked_past_its_documents_bound_is_not_indexed",
    },
    Property {
        family: "Verified client graph transaction",
        name: "nothing_without_authority / a_held_key_is_enough / a_foreign_signature_sends_nothing / another_nodes_delegation_sends_nothing / an_expired_credential_sends_nothing / a_request_sends_only_its_command / a_sent_request_names_its_command / a_request_is_no_general_authority / general_authority_covers_every_command / a_fresh_credential_is_taken_as_at_admission / a_fresh_request_for_another_command_is_refused / a_fresh_credential_of_another_signer_is_refused / a_fresh_request_releases_one_command / one_rule_for_every_form / an_edit_needs_the_permission / an_edit_is_admitted_on_fresh_evidence_only / stale_evidence_admits_nothing / an_unreachable_provider_admits_nothing / a_revocation_in_the_verified_revision_refuses / an_owed_revocation_refuses / an_owed_registration_admits_only_its_creator / a_request_never_admits_an_edit / no_credential_on_an_unproven_provider_needs_the_key / no_credential_is_admitted_on_the_permission / a_bad_delegation_admits_no_edit",
        axis: Lean,
        anchor: "crates/engine/vera/src/cosmos/dac.rs authority; crates/engine/vera/src/cosmos/dac/request_bound.rs request_bound; crates/engine/acp/src/external/owed.rs reauthorize; crates/engine/query/src/runner/remote_authority.rs edit_credential; crates/engine/query/src/txn/context.rs check_doc_access_with_prepared_inputs",
        model_ref: "VerifiedRemoteAuthority (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_ActorAuthority / actor_nac_required / actor_dac_required",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/transaction.rs trusted embedded relationship delegation; crates/entry-points/defra-node/tests/unit/delegated_relationship.rs embedded_delegation_matches_executed_lean_model",
        model_ref: "MC_DelegatedRelationship_{Green,Red_*}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/delegated_relationship.rs embedded_delegation_matches_executed_lean_model, selected_actor_nac_revocation_conflicts_with_node_owned_grant",
    },
    Property {
        family: "Transactional document relationships",
        name: "native administrator requires active transaction-bound NAC and atomic relation authority",
        axis: Tla,
        anchor: "crates/engine/acp/src/zanzibar/acp/administrative_relationship.rs; crates/entry-points/defra-node/src/acp_ops/transactional.rs relationship_in_txn",
        model_ref: "MC_AdministrativeRelationship_{Green,Red_*}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/administrative_relationship.rs native_node_admin_manages_foreign_relationships_and_rechecks_revocation",
    },
    Property {
        family: "Transactional document relationships",
        name: "mayAdmin / public_route_denied / inactive_denied / bypass_denied / operation_denied / unbound_nac_denied / unbound_dac_denied",
        axis: Lean,
        anchor: "crates/engine/acp/src/zanzibar/acp/administrative_relationship.rs; crates/entry-points/defra-node/src/acp_ops/transactional.rs relationship_in_txn",
        model_ref: "AdministrativeRelationship (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/administrative_relationship.rs administrative_relationship_admission_matches_executed_lean_model",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_Wake / INV_Abort / INV_Noop for projected relationship changes",
        axis: Tla,
        anchor: "crates/database/db/src/document_projection.rs notify_access_changed; crates/engine/datastore/src/multistore.rs on_commit",
        model_ref: "MC_ProjectionAccessNotification_{Green,Red_*}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/projection_access_notification.rs projected_relationship_changes_notify_armed_observer_only_after_commit",
    },
    Property {
        family: "Transactional document relationships",
        name: "inScope / empty_scope_denied / outside_rules_denied",
        axis: Lean,
        anchor: "crates/engine/acp/src/projection.rs",
        model_ref: "ProjectionRelationship (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/engine/acp/tests/projection_scope.rs projection_scope_matches_executed_lean_model",
    },
    Property {
        family: "Transactional document relationships",
        name: "exact relationship observation requires coherent authenticated evidence",
        axis: Lean,
        anchor: "crates/engine/vera/src/vera_rs/provider/exact_relationship.rs query",
        model_ref: "ExactRelationshipObservation (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera/src/vera_rs/provider/exact_relationship.rs a_proof_at_another_root_answers_nothing, only_a_verified_record_of_the_grant_answers, an_absent_owner_refuses_as_a_registered_document_does",
    },
    Property {
        family: "Transactional document relationships",
        name: "unauthorized_learns_nothing / local_answer_requires_authority",
        axis: Lean,
        anchor: "crates/engine/acp/src/zanzibar/acp/document_acp.rs observe_actor_relationship",
        model_ref: "ExactRelationshipObservation (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/acp/tests/exact_observation.rs owners_and_managers_observe_the_exact_tuple, an_unauthorized_caller_learns_nothing_about_the_document, a_wildcard_is_not_an_exact_actor; crates/entry-points/defra-node/tests/unit/delegated_nac.rs http_inspection_routes_apply_the_embedded_gates; crates/database/db/tests/access/external_drainer.rs an_exact_observation_waits_for_a_change_marked_in_flight; crates/database/db/tests/access/external_observation.rs an_unauthorized_caller_is_refused_alike_whatever_the_document_owes, the_owner_observes_and_the_creator_of_an_owed_registration_waits; crates/entry-points/defra-node/tests/unit/transactional_relationship.rs an_observation_in_a_transaction_sees_its_own_relationship_changes; crates/entry-points/defra-node/tests/embedded_vera.rs a_grant_staged_beside_its_document_lets_the_grantee_read_after_the_commit",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_Guards / INV_Revocation / INV_Rollback for scoped projection relationships",
        axis: Tla,
        anchor: "crates/database/db/src/document_projection.rs change_projected_relationship; crates/engine/acp/src/projection.rs in_scope; crates/engine/acp/src/outbox/staging.rs stage_prepared",
        model_ref: "MC_ProjectionRelationship_{Green,Green_*,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/unit/document_projector.rs scoped_projection_uses_owner_authority_without_read_bypass",
    },
    Property {
        family: "Transactional document relationships",
        name: "mayProject / mayStageExternal refuse every missing guard",
        axis: Lean,
        anchor: "crates/database/db/src/document_projection.rs change_projected_relationship; crates/engine/acp/src/outbox/staging.rs prepare_relationship, stage_prepared",
        model_ref: "ProjectionRelationship (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/unit/document_projector.rs scoped_projection_uses_owner_authority_without_read_bypass; crates/database/db/tests/access/external_prepared.rs the_owner_stages_its_prepared_change_with_its_authority_checked, a_manager_prepares_as_the_owner_does_and_nobody_else_does, a_preparation_is_taken_for_its_own_change_and_provider_only, a_delegation_bound_to_the_change_authorizes_that_change_only; crates/entry-points/defra-node/tests/embedded_vera/projection.rs a_projection_stages_through_vera_for_a_manager_in_a_transaction_and_for_the_node",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_Identity / INV_Authority: a projection acts as its request's actor, or the node",
        axis: Tla,
        anchor: "crates/database/db/src/document_projection.rs change_projected_relationship, projected_owner; crates/database/db/src/database/mod.rs reconcile_document_projector",
        model_ref: "MC_ProjectionActor_{Green,Red}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/unit/document_projector.rs scoped_projection_uses_owner_authority_without_read_bypass; crates/entry-points/defra-node/tests/embedded_vera/projection.rs a_projection_stages_through_vera_for_a_manager_in_a_transaction_and_for_the_node",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_NoStaleHeld: an observation never answers held over an owed revocation",
        axis: Tla,
        anchor: "crates/engine/acp/src/external.rs observe_actor_relationship",
        model_ref: "MC_ObservationSettlement_{Green,Red_Unlocked}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/access/external_observation.rs a_revocation_owed_when_the_provider_answered_is_not_settled_under_the_observation",
    },
    Property {
        // The registrations one transaction's creates owe an external
        // provider are settled together: an object the provider holds is
        // cleared unsent, one creator's go as one ordered list applied all or
        // none, and a refused list is sent again one registration at a time,
        // first to last, so each is answered as alone.
        family: "ACP outbox and owed registrations",
        name: "INV_ClearedOnlyHeld / INV_OwnerIsCreator / INV_RefusedOnlyIfBad / INV_NoResendOfKnownHeld / INV_HeldNotFailed / INV_Order: a record is cleared only once its object is held (for its creator, or for another node's owner it yields to), a list carries one creator, only a registration refused alone is parked, no request carries an object the settler found held, a refusal of an object another settler registered meanwhile is cleared as held, and a refused list's members go alone in order",
        axis: Tla,
        anchor: "crates/engine/acp/src/commit_registrations.rs CommitRegistrations::owe, settle, grouped; crates/engine/acp/src/external/registration_batch.rs settle_all_if_owed, to_send, send_group, Sender; crates/engine/acp/src/external/registration.rs held_after_refusal, registered; crates/engine/acp/src/external/replica.rs yield_replica_if_held_elsewhere; crates/engine/vera/src/cosmos/dac/registration_batch.rs register_listed, register_one_by_one; crates/engine/vera/src/vera_rs/provider/registration_list.rs calldata; crates/database/db/src/txn/mod.rs DbTxn::owe_acp_registration, OwedAcpRegistration::run; crates/database/db/src/collection/acp.rs collection_to_register",
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
        family: "Transactional document relationships",
        name: "INV_NotOlderThanOwed / INV_HoldsLatest / INV_EndsInTime / INV_ReuseHoldsLatest / INV_BackoffHeld: an observation waits, within one deadline, for its owed change",
        axis: Tla,
        anchor: "crates/engine/acp/src/external.rs observe_actor_relationship, observe_as_manager, await_tuple, before, OBSERVATION_PATIENCE; crates/database/db/src/document_projection/relationship_input.rs RelationshipInput; crates/engine/acp/src/outbox/ops.rs tuple_rest, tuple_head",
        model_ref: "MC_SettleBeforeObserving_{Green,Green_Silent,Red_NoWait,Red_EarlierOnly,Red_WakeFirst,Red_LockUnbounded,Red_NoExpiry,Red_NoExpiry_Silent,Red_WakeDrainer,Red_NoHead}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/acp/src/external/deadline_tests.rs a_passed_deadline_ends_a_wait_whose_future_is_ready, a_notification_before_the_first_poll_is_not_lost; crates/database/db/tests/access/external_observation.rs an_observation_waits_for_a_change_about_to_settle, an_observation_refuses_a_change_that_cannot_settle; crates/database/db/tests/access/external_observation_wait.rs notifications_of_other_records_do_not_extend_the_wait, a_settlement_pass_holding_the_lock_past_the_deadline_refuses_the_observation, a_caller_who_may_not_manage_is_refused_while_a_pass_holds_the_lock, an_observation_does_not_cut_the_drainers_backoff, the_tuple_head_moves_with_each_change_and_stays_once_it_settles; crates/database/db/src/document_projection/relationship_input.rs a_later_change_on_the_tuple_names_another_input; crates/entry-points/defra-node/tests/embedded_vera/settle_before_observing.rs creates_updated_at_once_under_load_project_without_refusal",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_NoEscalation / INV_NoOwner / projection_keeps_requester_authority",
        axis: Tla,
        anchor: "crates/database/db/src/document_projection.rs change_projected_relationship, DocumentMutationProjector::project_related",
        model_ref: "MC_ProjectionNoEscalation_{Green,Red_*}.cfg; ProjectionRelationship (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/unit/related_projection.rs related_projection_grants_only_what_the_requester_could_grant_directly",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_SignedOnly / INV_OptedIn / INV_RevocationStands for genesis read access",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/genesis_access.rs register_signed_genesis",
        model_ref: "MC_GenesisReadAccess_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/merge/genesis_access.rs a_revoked_initial_reader_stays_revoked_when_the_genesis_arrives_again, a_genesis_without_a_reader_registers_only_its_owner",
    },
    Property {
        family: "Transactional document relationships",
        name: "genesisReader / addressed_names_only_receiver / replay_keeps_revocation / creatorReader / creator_addressed_names_only_address / creator_matches_addressed_receiver",
        axis: Lean,
        anchor: "crates/database/db/src/merge/merge_handler/genesis_access.rs genesis_reader, creator_reader",
        model_ref: "EnrollmentGenesis (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/merge/genesis_access.rs genesis_reader_matches_executed_lean_model, only_an_immutable_address_naming_this_node_selects_it, creator_reader_matches_executed_lean_model, a_creator_grants_exactly_the_address_its_document_names",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_SameRoot / INV_FailsOnlyWhenEveryAskOvertaken for exact relationship observations",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/provider/exact_relationship.rs query",
        model_ref: "MC_ExactRelationshipObservation_{Green,Green_Exhausted,Red,Red_NoRetry}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera/src/vera_rs/provider/exact_relationship.rs a_proof_at_another_root_answers_nothing, an_overtaken_observation_is_asked_again_at_one_root, an_observation_overtaken_at_every_ask_ends_in_the_refusal; crates/entry-points/defra-node/tests/embedded_vera.rs an_observation_under_writes_is_asked_again_when_a_block_overtakes_it",
    },
    Property {
        family: "Transactional document relationships",
        name: "committed_grant_notifies / committed_revoke_notifies / abort_does_not_notify / unchanged_does_not_notify",
        axis: Lean,
        anchor: "crates/database/db/src/document_projection.rs notify_access_changed",
        model_ref: "ProjectionAccessNotification (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/projection_access_notification.rs projected_access_notifications_match_executed_lean_model",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_Membership / INV_Revocation for userset relation managers",
        axis: Tla,
        anchor: "crates/engine/acp/src/zanzibar/acp/mod.rs check_manage_relation; crates/entry-points/defra-node/src/acp_ops/transactional.rs relationship_in_txn",
        model_ref: "MC_UsersetRelationManager_{Green,Red_*}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/userset_manager.rs transactional_userset_managers_respect_owner_admin_scope_and_revocation",
    },
    Property {
        family: "Transactional document relationships",
        name: "owner_allowed / direct_allowed / wildcard_allowed / node_owner_allowed / node_admin_allowed / foreign_node_denied / undeclared_denied",
        axis: Lean,
        anchor: "crates/engine/acp/src/zanzibar/acp/mod.rs check_manage_relation",
        model_ref: "UsersetRelationManager (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/userset_manager.rs userset_manager_admission_matches_executed_lean_model",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_Authority / INV_Private / INV_NoRevival",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/acp_ops/transactional.rs; crates/entry-points/defra-node/src/ops/http_ops.rs; crates/entry-points/cli/src/doc_acp_adapter.rs",
        model_ref: "MC_TransactionalRelationship_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/unit/transactional_relationship.rs relationship_grant_and_revoke_obey_commit_and_discard, stale_marker_cannot_restore_revoked_relationship, failed_relationship_statement_does_not_leak_its_writes_or_events",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_NoFalseAuthority",
        axis: Tla,
        anchor: "crates/surfaces/http/src/handlers/acp.rs own_documents; crates/engine/query/src/runner/owners.rs observe_owners",
        model_ref: "MC_OwnerSelfObservation_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/unit/http_acp.rs http_owner_registration_matches_executed_model, http_relationships_and_owner_observation_use_the_authenticated_transaction",
    },
    Property {
        family: "Transactional document relationships",
        name: "INV_HttpOwnerIsolation / http_rejects_embedder / local_embedder_allowed",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/node_builder.rs build; crates/entry-points/defra-node/tests/unit/http_acp.rs http_cannot_use_the_embedder_only_transaction_registry",
        model_ref: "MC_HttpTxnConfiguration_{Green,Red_Allow}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/unit/http_acp.rs http_cannot_use_the_embedder_only_transaction_registry",
    },
    Property {
        family: "Transactional document relationships",
        name: "wrong_owner_refused / readonly_refused / node_denied / document_denied / neither_bound_nor_staging_refused / owed_staging_accepted / owed_staging_keeps_guards / expired_refused / unsupported_backend_refused",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/tests/unit/transactional_relationship.rs transaction_admission_matches_executed_lean_model; crates/entry-points/cli/tests/unit/doc_acp_adapter.rs a_named_transaction_never_falls_back_to_autocommit",
        model_ref: "TransactionalRelationship (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/transactional_relationship.rs transaction_admission_matches_executed_lean_model; crates/entry-points/cli/tests/unit/doc_acp_adapter.rs a_named_transaction_never_falls_back_to_autocommit",
    },
    Property {
        family: "HTTP client-signed create",
        name: "both_formats_admit_the_same_caller / expired_never_dispatches / permission_denied_never_dispatches",
        axis: Lean,
        anchor: "crates/surfaces/http/src/client_signing.rs upgrade; crates/surfaces/http/src/auth_middleware.rs auth_middleware; crates/engine/vera/src/vera_rs/bearer.rs caller_of_delegation",
        model_ref: "ClientSigningCredential (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/embedded_vera.rs a_caller_that_keeps_its_key_writes_through_its_own_delegation",
    },
    Property {
        family: "HTTP client-signed create",
        name: "INV_DelegationAdmitted / INV_CurrentAtDispatch",
        axis: Tla,
        anchor: "crates/surfaces/http/src/client_signing.rs upgrade and run",
        model_ref: "MC_SigningCredential_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/embedded_vera.rs a_caller_that_keeps_its_key_writes_through_its_own_delegation",
    },
    Property {
        family: "HTTP client-signed create",
        name: "full_refused / expired_has_no_lifetime / lifetime_bounded / lifetime_before_expiration",
        axis: Lean,
        anchor: "crates/surfaces/http/src/client_signing.rs upgrade; crates/surfaces/http/src/router/state.rs AppState",
        model_ref: "ClientSigningAdmission (lake build); client_signer_tests::admission_and_token_lifetime_match_the_model",
        tiers: &[Boundary, Contract],
        bound_by: "crates/surfaces/http/tests/unit/client_signer.rs admission_and_token_lifetime_match_the_model",
    },
    Property {
        family: "HTTP client-signed create",
        name: "reuse_requires_same_definition",
        axis: Lean,
        anchor: "crates/database/db/src/write/create.rs compute; crates/database/db/src/write/prepare.rs PreparedCreateBlocks",
        model_ref: "ClientCreateSchema (lake build); write::create::schema_binding_tests::prepared_create_checks_immutable_definition_even_when_version_is_unchanged",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/write/create_schema_binding.rs prepared_create_checks_immutable_definition_even_when_version_is_unchanged",
    },
    Property {
        family: "HTTP client-signed create",
        name: "INV_Capacity / LIVE_Reaped",
        axis: Tla,
        anchor: "crates/surfaces/http/src/client_signing.rs upgrade; crates/surfaces/http/src/router/state.rs AppState",
        model_ref: "MC_SigningAdmission_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/http/tests/unit/client_signer.rs admission_and_token_lifetime_match_the_model, disconnected_and_expired_connections_release_their_slots",
    },
    Property {
        family: "HTTP client-signed create",
        name: "original actor and context, verified waves, live operation, deciding ACP, outside-attempt interaction, and eventual cleanup",
        axis: Tla,
        anchor: "crates/surfaces/http/src/client_signing.rs run; crates/engine/query/src/runner/turn.rs drive; crates/database/db/src/write/create.rs compute",
        model_ref: "MC_ClientSignedCreate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/http_signing_tests.rs http_client_signs_the_complete_agent_create_graph; crates/surfaces/http/tests/unit/client_signer.rs signing_endpoint_requires_an_authenticated_caller, disconnected_and_expired_connections_release_their_slots",
    },
    Property {
        family: "HTTP client-signed create",
        name: "accepted_exact / null_cannot_be_omitted / generated_is_rejected",
        axis: Lean,
        anchor: "crates/database/db/src/block/builder/client_create.rs ClientCreateSignatures::matches_input",
        model_ref: "ClientCreateIntent (lake build); client_create_graph::normalized_agent_intent_preserves_explicit_null_and_datetime",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/block/client_create_graph.rs normalized_agent_intent_preserves_explicit_null_and_datetime",
    },
    Property {
        family: "HTTP client-signed create",
        name: "immutable_included / mutable_included / immutable_has_no_extra / mutable_has_no_extra / accepted_exact_links",
        axis: Lean,
        anchor: "crates/database/db/src/block/builder/immutable.rs immutable_fields; crates/database/db/src/block/builder/compute.rs compute_document_blocks; crates/database/db/src/block/builder/client_create.rs ClientCreateSignatures::build",
        model_ref: "ClientCreateGraph (lake build); client_create_graph::trusted_create_graph_matches_model_and_rejects_unapproved_waves",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/block/client_create_graph.rs trusted_create_graph_matches_model_and_rejects_unapproved_waves",
    },
    Property {
        family: "HTTP client-signed create",
        name: "accepted_live / accepted_exact_blocks / signed_exact_blocks",
        axis: Lean,
        anchor: "crates/surfaces/http/src/client_signing.rs run; crates/database/db/src/block/builder/client_waves.rs ClientWaves::respond",
        model_ref: "ClientSignedCreate (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/block/client_create_graph.rs trusted_create_graph_matches_model_and_rejects_unapproved_waves; crates/surfaces/http/tests/unit/client_signer.rs the_connection_signer_checks_each_signature_before_answering",
    },
    Property {
        family: "HTTP client-signed update",
        name: "fixed intent, live signing, relevant field and deciding authorization",
        axis: Tla,
        anchor: "crates/surfaces/http/src/client_signing.rs run; crates/database/db/src/block/builder/client_update.rs and update transactions",
        model_ref: "MC_ClientSignedUpdate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/http_signing_tests.rs http_client_signs_a_single_document_update, http_client_signed_update_rechecks_acp_and_cancellation",
    },
    Property {
        family: "HTTP client-signed update",
        name: "signed_document / signed_changes / signed_exact_wave / unchanged_skipped / immutable_refused / mutable_allowed",
        axis: Lean,
        anchor: "crates/database/db/src/block/builder/client_update.rs; crates/database/db/src/block/builder/client_waves.rs ClientWaves::respond",
        model_ref: "ClientSignedUpdate (lake build); client_create_graph::trusted_update_graph_matches_model_and_rejects_unapproved_waves",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/block/client_create_graph.rs trusted_update_graph_matches_model_and_rejects_unapproved_waves",
    },
    Property {
        family: "HTTP client-signed conditional update",
        name: "accepted_filter_has_approved_target",
        axis: Lean,
        anchor: "crates/surfaces/http/src/client_signed_update.rs single_document_target",
        model_ref: "ClientSignedUpdateFilter (lake build); single_document_filter_admission_matches_model",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/http/tests/unit/client_signed_update.rs single_document_filter_admission_matches_model",
    },
    Property {
        family: "SSE loss and error delivery",
        name: "INV_DetectedLossEnds / INV_ErrorForwarded / LossEndsStream",
        axis: Tla,
        anchor: "crates/foundation/events/src/channel_bus.rs ChannelBus::publish_raw; crates/surfaces/http/src/handlers/graphql/query.rs graphql_sse; crates/surfaces/http/tests/graphql_sse_recovery.rs",
        model_ref: "MC_SseDelivery_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/http/tests/graphql_sse_recovery.rs overflow_ends_the_graphql_stream_before_a_queued_result, query_error_is_delivered_even_without_rows",
    },
    Property {
        family: "SSE loss and error delivery",
        name: "loss_requires_recovery / loss_aware_closes / raw_stays_open / errors_are_forwarded / empty_is_skipped / denial_has_code / other_has_no_denial",
        axis: Lean,
        anchor: "crates/foundation/events/src/channel_bus.rs ChannelBus::publish_raw; crates/surfaces/http/src/handlers/graphql/query.rs graphql_sse; crates/engine/query/src/executor.rs QueryResponseError::from_query_error",
        model_ref: "SseDelivery (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/foundation/events/tests/unit/channel_bus.rs overflow_wakes_reader_that_drained_before_loss_was_recorded, ordinary_raw_subscription_remains_open_after_overflow; crates/surfaces/http/tests/graphql_sse_recovery.rs typed_permission_refusal_keeps_its_code_in_the_graphql_stream",
    },
    Property {
        family: "HTTP router composition",
        name: "WholeRouter / SingleConstructor / NoBypass / ForwardingPreserved",
        axis: Tla,
        anchor: "crates/surfaces/http/src/server.rs Server::router, Server::serve; router_transform_tests.rs forwarding_filter_matches_executed_model; crates/entry-points/defra-node/src/router_transform_tests.rs",
        model_ref: "MC_HttpRouterComposition_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "HTTP router composition",
        name: "configured_order / constructor_once / absent_preserves / forwarding_preserves / refuses_every_inner_status",
        axis: Lean,
        anchor: "crates/surfaces/http/src/server.rs Server::router; router_transform_tests.rs forwarding_filter_matches_executed_model",
        model_ref: "HttpRouterComposition (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Disconnect answers only once the peer's last connection closed, so
        // ActivePeers right after it no longer lists the peer, as in Go.
        family: "Peer connection observation",
        name: "INV_AnsweredMeansClosed / PROP_CallerAnswered",
        axis: Tla,
        anchor: "crates/database/p2p/src/host/command_handler/network.rs handle_disconnect; crates/database/p2p/src/host/p2p_host/swarm.rs handle_connection_closed; tools/integration-test/tests/testo/net/info/disconnect_peers.rs",
        model_ref: "MC_PeerDisconnect_{Green,Red_EarlyAck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // iroh answers Disconnect once the close has started, the Red_EarlyAck
        // shape: connected_peers lists the peer until its stream task observes
        // the close, so a connect_peer in that window does nothing. Callers
        // that need the peer gone poll connected_peers.
        family: "Peer connection observation",
        name: "INV_AnsweredMeansClosed binds libp2p only: iroh answers Disconnect before the peer leaves connected_peers",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_commands.rs handle_disconnect; crates/surfaces/p2p-adapter/src/iroh.rs connected_peers, connect_peer",
        model_ref: "MC_PeerDisconnect_Red_EarlyAck.cfg",
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
        family: "Peer connection observation",
        name: "ExplicitOnly / ExactAddress / RequestedPort / DefaultPreserved / UnavailableRefused / MappingScope / MappingDefaultsPreserved",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_config.rs apply_bind_config; endpoint_config/bind_tests.rs native_bindings_match_executed_model, native_port_mapping_configuration_matches_executed_model, loopback_endpoint_does_not_open_wildcard_gateway_discovery_listeners",
        model_ref: "MC_EndpointBind_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer connection observation",
        name: "EndpointBind.explicit_replaces / explicit_preserves / absent_preserves_defaults / port_only_preserves_dual_stack / specific_address_disables_mapping / absent_address_preserves_mapping / wildcard_preserves_mapping",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/endpoint_config.rs apply_bind_config; endpoint_config/bind_tests.rs model_cases, native_port_mapping_configuration_matches_executed_model",
        model_ref: "EndpointBind (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selective CAR capacity: observed completed grants yield to active sends; in-flight scope and quiescent limits are preserved",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/selective_car_access.rs register, reclaim_completed and allows_root",
        model_ref: "MC_SelectiveCarCapacity_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "admitted_bounded / warm_does_not_refuse / reclaim_preserves_active / authorized_scope / expired_completed_refused",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/selective_car_access.rs register, reclaim_completed and allows_root",
        model_ref: "SelectiveCarCapacity (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected transfer owns its acknowledgements independently of subscriptions; replay stays fenced, scoped and grant-bound",
        axis: Tla,
        anchor: "crates/database/db/src/merge/push_docs.rs push_existing_docs_by_id",
        model_ref: "MC_DocumentPush_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Revoke is API-only: revoke_selected_car_root has no production
        // caller. A grant lasts for the life of the store, through deny/allow,
        // purge and identity rotation, and storage grows with each distinct
        // selected (peer, head). A grant only re-authorizes bytes already sent
        // to that peer.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected exact peer/root authority survives cache loss, restart and time until explicit revocation",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/peerstore/selected_car.rs revoke_selected_car_root; crates/database/p2p/src/sync/coordinator/event_handler/car.rs",
        model_ref: "MC_SelectedCarAuthority_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected pushes ignore subscription filters and retry schedules, retain hint ownership, pause and retry the same head on capacity hints within its own first-send deadline, defer a head whose pauses outlast that deadline without stopping the rest, stop scheduling after a stall, disconnect or cancellation, bound late sends and timeouts by the send slots, drain all started tasks and succeed only when every document is acknowledged",
        axis: Tla,
        anchor: "crates/database/db/src/merge/push_docs.rs push_existing_docs_by_id; crates/database/db/src/merge/push_docs/stop.rs SelectedPushStop, stops_selected_transfer; crates/database/db/src/merge/push_docs/selected.rs HeadDeadline; crates/surfaces/p2p-adapter/src/pooled_doc_pusher.rs push_existing_docs_by_id",
        model_ref: "MC_SelectedPushIsolation_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // As above: nothing in production revokes, so revoked_denied is about
        // an API no caller uses.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "revoked_denied / other_preserved / exact_scope",
        axis: Lean,
        anchor: "crates/engine/storage/src/stores/peerstore/selected_car.rs",
        model_ref: "SelectedCarAuthority (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "selection_owns_acknowledgements / absent_replay_is_refused / selected_failure_is_not_deferred / selected_retains_heads / absent_peer_has_no_observation / no_progress_stops / pause_expiry_does_not_stop / expired_head_leaves_the_rest / stopped_admits_nothing / stopped_selection_fails / selected_success_needs_every_acknowledgement",
        axis: Lean,
        anchor: "crates/database/db/src/merge/push_docs.rs; crates/database/db/src/merge/push_docs/stop.rs; crates/database/db/tests/merge/push_docs_selection.rs",
        model_ref: "DocumentPush.lean; selected_document_transfers_match_executed_model",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Rooted authorization: one producer, exact grants, bounded optional retention, fenced eviction and cancellation wakeup",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/car_authorization.rs; car_authorization_registry.rs walk_with_limits; crates/database/p2p/tests/unit/car_authorization_limits.rs",
        model_ref: "MC_RootedAuthorization_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "reserve_bounded / release_restores / replacement_survives / grants_independent_of_retention",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/car_authorization_budget.rs; car_authorization_registry.rs",
        model_ref: "RootedAuthorization (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Replicator installs validate their row, settle before completion, and retain interrupted outcomes",
        axis: Tla,
        anchor: "crates/surfaces/p2p-adapter/src/replicator_install_store.rs; crates/surfaces/p2p-adapter/src/replicator_installs.rs",
        model_ref: "MC_ReplicatorInstall_{Green,Red_*}.cfg; MC_ReplicatorIncarnation_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Deferred replay preserves its install identity, cannot recreate or alter a replaced peer row, flips its own install's row in one transaction whatever same-incarnation rewrite came first, writes retry markers only for its own install, and stops recording failures only when that install is replaced",
        axis: Tla,
        anchor: "crates/database/db/src/merge/push_docs_replay.rs persist_replay_failures; crates/database/db/src/merge/push_docs.rs push_existing_docs_with_config_and_allowlist; crates/engine/storage/src/stores/peerstore.rs update_replicator_for_incarnation, observe_push_head_for_incarnation, record_push_failure_for_incarnation; crates/surfaces/p2p-adapter/src/replicator_status.rs set_persisted_replicator_status_for_incarnation_with_marker_state; crates/surfaces/p2p-adapter/src/replicator_install_store.rs finish",
        model_ref: "MC_ReplicatorIncarnation_{Green,Red_Replay*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "A live write whose marker a newer generation supersedes (a replay's or a second live writer's) is owed by that marker and never reported undurable, and its head stays delivered or marked, because a live ack completes the marker only at its own generation and only while its head is the whole frontier, on the retired branch too",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/peerstore.rs register_scope_marker, register_scope_marker_at; crates/database/db/src/merge/push_docs.rs push_existing_docs_with_config_and_allowlist; crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job, is_whole_frontier",
        model_ref: "MC_LiveMarkerGeneration_{Green,Red_Superseded,Red_NoFence,Red_NoHeadFence}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/storage/src/stores/peerstore_tests.rs live_registration_superseded_by_replay_marker_is_owed_by_it; crates/database/p2p/src/sync/coordinator/push_worker_tests.rs retired_older_head_with_newer_generation_leaves_the_marker",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "defer_preserves_incarnation / defer_does_not_resurrect / stale_defer_leaves_replacement / fenced_preserves_incarnation / create_changes_incarnation / create_resurrects / stale_mark_writes_nothing / deleted_mark_writes_nothing / mark_owned / only_replacement_stops / superseded_continues",
        axis: Lean,
        anchor: "crates/database/db/src/merge/push_docs_replay.rs persist_replay_failures; crates/engine/storage/src/stores/peerstore.rs record_push_failure_for_incarnation",
        model_ref: "ReplicatorInstall (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Install completion is identity-fenced and idempotent; stale allocations re-decide without regressing the incarnation",
        axis: Lean,
        anchor: "crates/surfaces/p2p-adapter/src/replicator_install_store.rs finish",
        model_ref: "ReplicatorInstall (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_Released — an early reconciliation return releases its queued continuation slot",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/replication/handlers.rs::process_one_event; crates/database/p2p/src/sync/replication/continuation_tests.rs::already_merged_duplicate_continuation_releases_reservation_without_handler",
        model_ref: "MC_ContinuationSlot_{Green,Red_LateClaim}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_Capacity / INV_OwnedSlot / INV_NoIdleCapacity / INV_RollbackNeedsCause — only an owner's own generation releases its pending-DAG slot, the release wakes the retry clock, and a full-lane rollback does not",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs PendingDagWorkOwner, claim_pending_dag_merge, spawn_pending_dag_fetch, dispatch_due_pending_dag_fetches and run_pending_dag_retry_clock; crates/database/p2p/tests/unit/pending_capacity_wake.rs",
        model_ref: "MC_PendingDagOwner_{Green,Red_NoGeneration,Red_NoWake,Red_RollbackWake}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "owned_slot_released — dropping the continuation owner releases its slot",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs::claim_pending_dag_merge; crates/database/p2p/src/sync/replication/handlers.rs::process_one_event",
        model_ref: "HistoryTurns.owned_slot_released (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Rejected-page cleanup compares the cursor incarnation before removing it",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/history.rs::reject_history; crates/database/db/tests/merge/history_cursor.rs",
        model_ref: "MC_RetryMarkerGeneration_{Green,Red_StaleAck,Red_Readd}.cfg (incarnation-fenced retirement abstraction)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "History turns commit cursor and writes together; reclaim preserves publications",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/history.rs; history_registry.rs; dispatch.rs; crates/database/db/src/merge/replication.rs; crates/database/db/tests/merge/collection_history_control.rs",
        model_ref: "MC_HistoryTurns_{Green,Red_*}.cfg; MC_HistoryDriver_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Ancestor publication is idempotent; cleanup and collection carriers preserve driver ownership",
        axis: Lean,
        anchor: "crates/database/db/src/merge/merge_handler/history.rs",
        model_ref: "HistoryTurns (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
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
        family: "Peer connection observation",
        name: "NoSpuriousDenial — queued lifecycle notification cannot hide a connected peer",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/authorizer.rs RuntimeAuthorizer::peer_connected; access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
        model_ref: "MC_PeerConnection_Green.cfg / MC_PeerConnection_Red_StaleObservation.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/src/sync/coordinator/access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
    },
    Property {
        family: "Peer connection observation",
        name: "PeerConnection.live_is_connected / connection_does_not_grant_push",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/authorizer.rs RuntimeAuthorizer::peer_connected",
        model_ref: "PeerConnection.lean",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/src/sync/coordinator/access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
    },
    Property {
        family: "Peer connection observation",
        name: "Reverse RPC reuses inbound and legacy paths; stale eviction cannot hide replacements from disconnect",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_streams.rs adopt_new_connection; endpoint_rpc.rs remember_connection, connect_with_cache and evict_connection; crates/database/p2p/tests/iroh/shared_connection.rs",
        model_ref: "MC_SharedRpcConnection_{Green,Red_*}.cfg; MC_LegacyRpcCache_{Green,Red_*}.cfg; MC_RpcCacheEviction_{Green,Red_*}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/tests/iroh/shared_connection.rs disconnect_closes_replacement_despite_stale_cleanup, legacy_only_peer_reuses_and_refreshes_its_shared_rpc_path, accepted_inbound_connection_serves_reverse_rpc_without_another_dial, authenticated_reconnection_replaces_an_open_old_rpc_path",
    },
    Property {
        // When the cached RPC handle closes, its slot is left empty and the
        // next RPC dials rather than promoting an open sibling. That is cheap
        // only because every dialled connection is adopted and served; a
        // sibling of unknown liveness may be a connection whose peer is gone.
        family: "Peer connection observation",
        name: "A closed cached RPC connection is replaced by a fresh dial, never by promoting a sibling",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_rpc.rs connect_with_cache, evict_connection; crates/database/p2p/src/iroh/endpoint_streams.rs handle_connection_streams",
        model_ref: "MC_SharedRpcConnection_{Green,Red_NoAdoption}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer connection observation",
        name: "Every new connection is served, announced by its pump and released once; an allowlist miss keeps the sibling",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_streams.rs adopt_new_connection and finish_connection; endpoint_rpc.rs connect_with_cache and restore_displaced_connection; endpoint_commands.rs handle_dial; crates/database/p2p/tests/iroh/shared_connection.rs",
        model_ref: "MC_ConnectionAdoption_{Green,Red_*}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/tests/iroh/shared_connection.rs repeated_cache_hits_reuse_one_adopted_connection, closed_registry_refuses_adoption_and_balances_the_peer_map, cancelled_adopt_still_announces_and_releases_its_connection, allowlist_miss_refuses_only_the_new_connection_and_spares_the_sibling",
    },
    Property {
        family: "Peer connection observation",
        name: "Open RPC connections stay counted after cache displacement and close on revoke",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_rpc.rs close_peer_connections; crates/database/p2p/tests/iroh/shared_connection.rs displaced_rpc_dialed_connection_is_closed_by_revoke",
        model_ref: "MC_ConnectionAdoption_{Green,Red_*}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/tests/iroh/shared_connection.rs displaced_rpc_dialed_connection_is_closed_by_revoke",
    },
    Property {
        family: "Peer connection observation",
        name: "SharedRpc replacement identity, legacy reuse, modern preference, revoked refusal, stable-id eviction and refused-publish restore",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/endpoint_rpc.rs remember_connection, restore_displaced_connection, connect_with_cache and evict_connection; endpoint_streams.rs adopt_new_connection",
        model_ref: "PeerConnection.SharedRpc",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/tests/iroh/shared_connection.rs authenticated_reconnection_replaces_an_open_old_rpc_path, legacy_only_peer_reuses_and_refreshes_its_shared_rpc_path, legacy_mux_does_not_replace_the_modern_shared_rpc_handle, disconnect_closes_replacement_despite_stale_cleanup",
    },
    Property {
        family: "Schema publication completion",
        name: "VisibleBeforeReply — committed schemas are published before their callers resume",
        axis: Tla,
        anchor: "crates/engine/storage/src/corekv/callbacks.rs OnCommit::OrderedAndWait; crates/database/db/tests/collection/schema_publication.rs",
        model_ref: "MC_SchemaPublication_Green.cfg / MC_SchemaPublication_Red_EarlyReply.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/collection/schema_publication.rs schema_success_waits_for_its_ordered_cache_publication",
    },
    Property {
        family: "Schema publication completion",
        name: "SchemaPublication.preserves",
        axis: Lean,
        anchor: "crates/engine/storage/src/corekv/callbacks.rs OnCommit::OrderedAndWait; crates/database/db/tests/collection/schema_publication.rs",
        model_ref: "SchemaPublication.lean",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/collection/schema_publication.rs schema_success_waits_for_its_ordered_cache_publication",
    },
    Property {
        family: "DiskANN pending cache",
        name: "INV_NoHiddenPending — a search using the in-memory pending list never misses a vector its snapshot or its own transaction has pending",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/engine/diskann/graph.rs pending_candidates; crates/database/db/src/index/vector/engine/diskann/pending_cache.rs PendingCache; crates/database/db/src/index/vector/engine/diskann/mod.rs insert, delete; crates/database/db/src/index/vector/engine/diskann/merge.rs merge_plan, merge",
        model_ref: "MC_PendingCache_Green.cfg (GREEN) / MC_PendingCache_Red_NoGeneration.cfg / MC_PendingCache_Red_OwnWrites.cfg / MC_PendingCache_Red_TruncateThroughHead.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_pending_cache.rs later_inserts_catch_the_copy_up_without_a_reload, a_transaction_finds_its_own_uncommitted_insert, a_merge_truncates_the_log_it_absorbed, an_insert_during_a_merge_stays_pending",
    },
    Property {
        family: "DiskANN pending cache",
        name: "prefix_covered — a cache loaded once and caught up on every logged insert holds every vector a snapshot of its generation has pending, and a merge that cuts the log anywhere loses no live vector",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/pending_cache.rs PendingCache; crates/database/db/src/index/vector/engine/diskann/mod.rs insert; crates/database/db/src/index/vector/engine/diskann/merge.rs merge_plan, merge",
        model_ref: "PendingCache.Covers.prefix_covered / catch_up_in_steps / merge_keeps_every_vector / truncate_through_head_loses",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_pending_cache.rs later_inserts_catch_the_copy_up_without_a_reload, a_catch_up_past_the_cap_is_refused, a_merge_truncates_the_log_it_absorbed",
    },
    Property {
        family: "DiskANN pending cache",
        name: "INV_ScoresLatest — a search scores each pending vector by the newest entry its snapshot or its own transaction holds, never an obsolete one or one from a cache ahead of it",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/engine/diskann/graph.rs pending_candidates, snapshot_pending; crates/database/db/src/index/vector/engine/diskann/pending_cache.rs Copy::for_each_latest_through, Copy::caught_up, Segment::push, Segment::supersede",
        model_ref: "MC_PendingCache_Green.cfg (GREEN) / MC_PendingCache_Red_EveryVersion.cfg / MC_PendingCache_Red_FutureVersion.cfg / MC_PendingCache_Red_HeadBlindMarks.cfg / MC_PendingCache_Red_NoGapCheck.cfg / MC_PendingCache_Red_NoGeneration.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_pending_cache.rs an_updated_vector_is_scored_by_its_latest_code, a_transaction_scores_its_own_update_by_its_latest_code, a_snapshot_behind_the_copy_scores_the_codes_it_can_see, a_superseded_marker_respects_the_readers_head",
    },
    Property {
        family: "DiskANN pending cache",
        name: "scored_is_last — the code kept for a vector is that of its last entry visited, and a copy cut at a search's head scores as the search's own log does",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/graph.rs pending_candidates; crates/database/db/src/index/vector/engine/diskann/pending_cache.rs Copy::for_each_latest_through",
        model_ref: "PendingCache.Latest.scored_is_last / scored_append / cut_scores_snapshot / uncut_scores_future",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_pending_cache.rs an_updated_vector_is_scored_by_its_latest_code, a_snapshot_behind_the_copy_scores_the_codes_it_can_see",
    },
    Property {
        family: "DiskANN pending cache",
        name: "kept_is_newest — a search that skips an entry only when its superseded marker is at or before the search's head keeps exactly each vector's newest entry in reach",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/pending_cache.rs Segment::push, Segment::supersede, Copy::for_each_latest_through",
        model_ref: "PendingCache.Marks.kept_is_newest / newest_is_kept / one_kept_per_vector / next_decides / head_blind_marker_hides",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_pending_cache.rs a_superseded_marker_respects_the_readers_head, racing_catch_ups_agree_on_markers, a_folded_copy_keeps_one_entry_per_vector",
    },
    Property {
        family: "DiskANN pending cache",
        name: "Evict — forgetting the pending cache at any moment, as an index or collection drop does, keeps every search's candidates complete and scored by their newest entry",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/engine/diskann/cache.rs CodeCaches::evict, evict_collection; crates/database/db/src/collection/ops/delete.rs delete_collection_as; crates/database/db/src/collection/ops/version.rs delete_collection_version_as",
        model_ref: "MC_PendingCache_Green_Evict.cfg (GREEN)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_pending_cache.rs evicting_drops_only_the_named_caches",
    },
    Property {
        family: "DiskANN shared quantizer",
        name: "INV_EncodesWithItsCodebooks — a write sharing the node's quantizer encodes only with its own snapshot's codebooks, across rebuilds and retried trainings",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/engine/diskann/mod.rs DiskAnn::quantizer; crates/database/db/src/index/vector/engine/diskann/cache.rs CodeCache::quantizer, CodeCache::share_quantizer; crates/database/db/src/index/vector/engine/diskann/build.rs train_and_build",
        model_ref: "MC_SharedQuantizer_Green.cfg (GREEN) / MC_SharedQuantizer_Red_NoTagCheck.cfg / MC_SharedQuantizer_Red_TagPerRequest.cfg / MC_SharedQuantizer_Red_OneSlot.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_shared_quantizer.rs a_write_shares_the_quantizer_under_its_training, a_quantizer_under_another_training_is_not_used, writers_straddling_a_rebuild_keep_both_quantizers",
    },
    Property {
        family: "DiskANN shared quantizer",
        name: "decode_encode — the trained record reads back as written in both encodings, and the shared quantizer answers a tag only with that tag's codebooks",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/codec.rs encode_trained, decode_trained; crates/database/db/src/index/vector/engine/diskann/cache.rs CodeCache::quantizer, CodeCache::share_quantizer",
        model_ref: "SharedQuantizer.Tagged.decode_encode / lookup_is_books / share_keeps_consistent / reused_tag_serves_other_books / share_keeps_previous / one_slot_evicts",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_shared_quantizer.rs trained_record_reads_back_with_and_without_its_training, a_quantizer_under_another_training_is_not_used",
    },
    Property {
        family: "RobustPrune blocked covered checks",
        name: "blocked_eq_sequential — checking candidates in blocks keeps exactly the edges the one-at-a-time walk keeps, for any cut into blocks",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/select/robust_prune.rs RobustPrune::select; crates/foundation/defra-core/src/vector/kernel/mod.rs squared_euclidean_block",
        model_ref: "RobustPruneBlocks.blocked_eq_sequential / no_since_keeps_a_covered_edge",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/edge_selection.rs robust_prune_keeps_the_edges_one_at_a_time_checking_keeps",
    },
    Property {
        family: "DiskANN back-edges in place",
        name: "in_place_eq_decoded — adding back-edges to an encoded record in place leaves the entries and count that decoding, adding and encoding would",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/codec.rs add_edges; crates/database/db/src/index/vector/engine/diskann/graph.rs add_back_edges",
        model_ref: "BackEdgesInPlace.in_place_eq_decoded / stale_search_duplicates_an_edge",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_back_edges.rs adding_edges_in_place_writes_what_decoding_and_encoding_would",
    },
    Property {
        family: "Vector index link maintenance",
        name: "INV_NoLostLinks — blind additions preserve links and stale structural pruning aborts",
        axis: Tla,
        anchor: "crates/engine/storage/src/backends/regolith/merge/vector.rs; crates/database/db/src/index/vector/engine/hnsw/insert.rs; crates/database/db/tests/vector/concurrent_hnsw.rs",
        model_ref: "MC_VectorLinks_Green.cfg / MC_VectorLinks_Red_Puts.cfg / MC_VectorLinks_Red_Prune.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/concurrent_hnsw.rs overlapping_inserts_keep_shared_links_and_metadata_counts, structural_pruning_conflicts_with_a_new_link",
    },
    Property {
        // Two writers appending to one collection without seeing each other must
        // both land, leaving two tips for a later merge: that is what makes the
        // head set a CRDT rather than a last-writer-wins cell. The eager-delete
        // strategy makes them write the same key, which regolith refuses at
        // every isolation level, so the RED configuration is the defect. The
        // GREEN one is the per-key design that preceded the head-set
        // operator; the tree reaches the same conclusion by blind merges into
        // one key, and overlapping_appends_both_commit checks it.
        family: "Concurrent collection-head transitions",
        name: "INV_NoWriteConflict — concurrent appends to one collection both commit and form siblings",
        axis: Tla,
        anchor: "crates/database/db/src/block/builder/collection.rs write_collection_block; crates/database/db/src/block/heads.rs publish_collection_block; crates/engine/storage/src/backends/regolith/transaction.rs; crates/database/db/tests/block/heads.rs overlapping_appends_both_commit",
        model_ref: "MC_HeadSet_Green.cfg (GREEN) / MC_HeadSet_Red_EagerDelete.cfg (RED) / MC_HeadSet_Red_PerKeyScan.cfg (RED)",
        tiers: &[Behavioral],
        bound_by: "crates/database/db/tests/block/heads.rs overlapping_appends_both_commit",
    },
    Property {
        // A replicated block is published into its document's head set once:
        // its priority index entry records the publication, so a replay never
        // makes a superseded head live again, and a parent held ahead is read
        // by its own part of the set, so the parent's concurrent publication
        // (and nothing unrelated) refuses that decision. No guard key exists.
        family: "Concurrent collection-head transitions",
        name: "INV_LiveExact / INV_AheadExact / INV_NoUnrelatedAbort — document head publication without guard keys",
        axis: Tla,
        anchor: "crates/database/db/src/block/heads/document.rs publish/was_published; crates/engine/storage/src/backends/regolith/merge/head_set.rs part/touches; crates/database/db/tests/block/document_heads.rs, crates/database/db/tests/merge/head_publication.rs",
        model_ref: "MC_HeadPublication_Green.cfg (GREEN) / MC_HeadPublication_Red_{NoRecord,OwnUnvalidated,NoPartRead,WholeSet}.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/block/document_heads.rs replay_of_a_superseded_parent_cannot_resurrect_it, holding_a_parent_ahead_conflicts_with_its_publication, an_unrelated_publication_does_not_turn_away_a_hold; crates/database/db/tests/merge/head_publication.rs a_replayed_create_after_a_local_update_stays_superseded",
    },
    Property {
        // The algebra the temporal model rests on, for the per-key design:
        // every key a writer writes names that writer, so two writers cannot
        // collide (disjointness), and the eager strategy demonstrably does
        // collide (non-vacuity). The tree keeps none of those keys; it binds to
        // this model only at reads, where head_queries_match_executable_lean
        // compares derivedHeads with live_collection_heads.
        family: "Concurrent collection-head transitions",
        name: "derivedHeads reads as live_collection_heads; derived_writeSets_disjoint / eager_writeSets_overlap / applyDerived_parents_not_head are about the per-key design",
        axis: Lean,
        anchor: "crates/database/db/src/block/heads.rs live_collection_heads, publish_collection_block; crates/database/db/src/block/builder/collection.rs write_collection_block; crates/database/db/tests/block/head_conformance.rs head_queries_match_executable_lean",
        model_ref: "HeadSet.Core (lake build)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/block/head_conformance.rs head_queries_match_executable_lean",
    },
    Property {
        family: "Commit-ordered document arrivals",
        name: "dense positions / commit order / at-most-once / snapshot prefix",
        axis: Tla,
        anchor: "crates/database/db/src/event/arrivals.rs record; crates/engine/storage/src/backends/regolith/transaction.rs append; crates/database/db/tests/read/arrival_transactions.rs",
        model_ref: "regolith@e844baefe838f3af76403afd46c4b42f1a0b98a5 proofs/tla/CommitOrderedAppend.tla (GREEN Groups/Pipeline/Faults, RED Counter/OnceFromView/AssignBeforeValidation)",
        tiers: &[Behavioral],
        bound_by: "crates/database/db/tests/read/arrival_transactions.rs reversed_concurrent_commits_receive_cursors_in_commit_order, rollback_is_atomic_and_duplicate_arrivals_do_not_advance, cursor_survives_database_reopen",
    },
    Property {
        family: "Committed document update discovery",
        name: "INV_Atomic / INV_Once / INV_OnlyChanges / INV_Authorized / INV_DeletedHidden / INV_UnregisteredAfterDelete / INV_CursorBound / INV_NoSkippedVisible — a block that changed its document is journaled once in its transaction, one that lost to the stored value is not; a page of up to Limit rows is decided at its own snapshot, or confirmed after a provider outside its transaction answered, which loses a registration only after this node's delete; a deleted document's rows are shown to nobody. Not modeled: the reader's cursor-beyond-head and missing-row errors. Not prevented: a registration a peer took from a shared provider (Red_Unordered)",
        axis: Tla,
        anchor: "crates/database/db/src/event/updates.rs record/read/rows/drain; crates/database/db/src/event/updates/gate.rs Gate::decide, Gate::confirm; crates/database/db/src/txn/mod.rs DbTxn::later; crates/database/db/src/docid/map.rs peek_doc_short_id; crates/database/db/src/write/update.rs write_blocks; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document; crates/database/db/src/merge/merge_handler/field_state.rs merge_field_state; crates/engine/query/src/runner/arrivals.rs execute_arrivals_query",
        model_ref: "MC_DocumentUpdateJournal_{Green,Green_Outside}.cfg / MC_DocumentUpdateJournal_Red_{Detached,Duplicate,Superseded,Authorization,Deleted,Stale,Unordered,Skip}.cfg",
        tiers: &[Behavioral],
        bound_by: "crates/database/db/tests/read/update_transactions.rs commit_order_rollback_and_replayed_updates, updates_and_replay_checkpoint_survive_reopen, a_scan_returns_the_rows_of_its_collection_and_range_only, a_scan_is_closed_when_a_row_is_missing; crates/database/db/tests/read/arrivals.rs graphql_updates_page_each_committed_version_of_one_document, hidden_updates_advance_page_without_disclosing_identity_or_cid; crates/database/db/tests/read/update_visibility.rs a_deleted_private_documents_updates_are_shown_to_nobody, a_purged_private_documents_updates_are_shown_to_nobody, one_page_decides_each_document_once, a_page_in_a_transaction_decides_at_that_transactions_snapshot, a_page_in_a_writing_transaction_does_not_fail_it_when_its_document_is_purged, a_delete_during_the_page_hides_the_row_from_a_provider_outside_its_transaction, a_transaction_fetcher_pages_for_a_provider_outside_its_transaction; crates/database/db/tests/read/update_visibility/transaction_fetcher.rs a_delete_during_the_page_hides_the_row, a_page_in_a_writing_transaction_reads_its_later_snapshot_too, a_transaction_no_database_began_says_why_it_cannot_confirm; crates/database/db/tests/read/update_page.rs a_page_reads_its_rows_in_one_scan; crates/database/db/tests/docid/map.rs a_peek_resolves_what_a_get_resolves; crates/database/db/tests/merge/merge_handler_tests.rs peer_update_is_discovered_once_by_committed_cursor; crates/database/db/tests/merge/update_journal.rs a_merge_that_loses_to_the_stored_value_adds_no_row, a_block_merged_again_by_another_handler_adds_no_second_row, a_block_whose_field_was_merged_on_its_own_first_still_gets_its_row",
    },
    Property {
        family: "Committed document update discovery",
        name: "append_idempotent / different_blocks_are_distinct / superseded_adds_no_row / unauthorized_not_visible / deleted_not_visible / confirmed_rows_are_allowed / page_within_limit / pages_join / once_key_injective",
        axis: Lean,
        anchor: "crates/database/db/src/event/updates.rs record/rows/once_key; crates/database/db/src/event/updates/gate.rs Gate::decide, Gate::confirm; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document",
        model_ref: "DocumentUpdateJournal (lake build)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/read/update_transactions.rs identities_are_unambiguous_and_payloads_round_trip, commit_order_rollback_and_replayed_updates, a_scan_returns_the_rows_of_its_collection_and_range_only; crates/database/db/tests/read/arrivals.rs graphql_updates_page_each_committed_version_of_one_document, hidden_updates_advance_page_without_disclosing_identity_or_cid; crates/database/db/tests/read/update_visibility.rs a_deleted_private_documents_updates_are_shown_to_nobody, a_delete_during_the_page_hides_the_row_from_a_provider_outside_its_transaction; crates/database/db/tests/merge/update_journal.rs a_merge_that_loses_to_the_stored_value_adds_no_row",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_ProviderOncePerContext / INV_NewRegistrationSurvivesOldAck / ACT_NewRegistrationSurvivesOldClear",
        axis: Tla,
        anchor: "crates/engine/query/src/error.rs QueryError::NeedsInput; crates/database/db/src/write/autocommit/create.rs create_many_with_resolved_inputs; update.rs update_with_projected_resolved_inputs; delete.rs delete_with_resolved_inputs; crates/database/db/src/write/create.rs embed, compute_many; crates/engine/query/src/prepared.rs; crates/engine/storage/src/stores/peerstore.rs register/complete/clear retry scope",
        model_ref: "MC_RetryPreparedInputs_{Green,Red_*}.cfg / MC_RetryMarkerGeneration_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_SweepReadsBounded / LIVE_VanishedMarkerDischarged — a sweep tick reads one probe and at most one page of markers; a purged document's marker is discharged under scheduler fairness only",
        axis: Tla,
        anchor: "crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass; crates/engine/storage/src/stores/peerstore.rs get_retry_page, load_scope_markers, reschedule_retry_peer_for_incarnation; crates/database/db/src/merge/push_docs.rs retry_doc; crates/database/db/src/merge/push_docs_common.rs complete_document_retry_if_absent",
        model_ref: "MC_RetryMarkerGeneration_{Green,Red_UnboundedReads,Red_KeepVanished,Red_NoRotation}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_QueuedPushOwnsLiveMarker — admission requires current durable ownership and elapsed backpressure",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/peerstore.rs admit_push_scope; crates/surfaces/p2p-adapter/src/retry.rs spawn_failure_recorder; crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job",
        model_ref: "MC_RetryPushAdmission_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "ACT_RegistrationCannotCrossFence — allocated registrations preserve peer incarnation and marker order",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/peerstore.rs RegisterScopeMarkerAttempt; create_replicator",
        model_ref: "MC_RetryRegistrationFence_{Green,Red_NoEpoch,Red_NoOrder}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_FanOutPeerLocal — one replicator's rejected registration leaves every other replicator's marker and job",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/broadcast.rs admit_replicator_push; broadcast_tests.rs fanout_registration_failure_is_peer_local",
        model_ref: "MC_RetryRegistrationFence_{Green,Red_FanOutAbort}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // One retry clock per peer serves all of its markers. An acknowledged
        // delivery, live, fenced by a newer marker generation, or replayed,
        // returns it to the first rung, never later than it was, so the next
        // failure charges the second rung; a new marker and a reconnect keep
        // the rung, so a peer that keeps failing climbs the ladder however
        // often local writes touch it; and the sweep reads a deadline past
        // the longest rung, left by a backward clock step, as due.
        family: "Retry input and marker fencing",
        name: "ACT_DeliveryRestarts / ACT_SilentPeerBacksOff / INV_HeldWithinLongestRung — a delivery restarts the peer clock, nothing else does, and no deadline holds past the longest rung",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/retry_info.rs RetryInfo::restart_after_delivery, RetryInfo::bounded_by, RetryInfo::bump_with_schedule; crates/engine/storage/src/stores/peerstore.rs PeerstoreRetryAttempt::{CompleteMarker, Reschedule, Restart, Activate}, RegisterScopeMarkerAttempt, Peerstore::load_scope_markers; crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass, record_failure; crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job; crates/database/db/src/merge/push_docs_common.rs complete_document_retry_if_current",
        model_ref: "MC_RetrySchedule_{Green,Red_KeepRung,Red_SupersededKeepsRung,Red_ReplayKeepsRung,Red_RegisterRestarts,Red_ActivateRestarts,Red_ClockBackHolds}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "restart_rung_le_one / restart_never_later / restart_within_first_rung / restart_respects_backpressure / bump_climbs",
        axis: Lean,
        anchor: "crates/engine/storage/src/stores/retry_info.rs RetryInfo::restart_after_delivery, RetryInfo::bump_with_schedule",
        model_ref: "RetrySchedule (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Collection schema-version OCC",
        name: "INV_NoStaleSchemaWrite — a writer cannot commit against an obsolete structural version",
        axis: Tla,
        anchor: "crates/database/db/src/txn/mod.rs validate_collection_for_write; crates/database/db/src/write/autocommit/{create,update,delete}.rs; crates/database/db/src/definition structural updates",
        model_ref: "MC_CollectionVersionOCC_Green.cfg / MC_CollectionVersionOCC_Red_NoValidation.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Explicit transaction statement atomicity",
        name: "INV_AbortedEffectsRemoved / INV_SuccessfulEffectsRetained / INV_NoCommitWhileCleanupPending",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/mutation.rs explicit transaction mutation execution; crates/database/db/src/txn/mod.rs statement savepoint; crates/engine/storage/src/backends/regolith/transaction.rs savepoint callbacks",
        model_ref: "MC_StatementAtomicity_Green.cfg / MC_StatementAtomicity_Red_*.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "B3 filtered replication",
        name: "INV_DagComplete — filtered push still converges per-document",
        axis: Tla,
        anchor: "crates/database/p2p filtered replication; crates/database/db (merge)",
        model_ref: "MC_S4_ModelB.cfg / M1Convergence.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/replication.rs two_node_replication_converges",
    },
    Property {
        // Head rediscovery is embedder-owned. Only sender retry markers and
        // receiver durable pending records re-drive. A head nobody holds an
        // obligation for (a gossip drop, a publish while offline, an
        // acked-then-dropped head) returns only through an explicit
        // sync_documents/BranchableSync or a replicator re-set. replication.rs
        // converges under a configured replicator; partition.rs converges after
        // an explicit replicator re-set.
        family: "DAG convergence (partition / eviction / restart)",
        name: "INV_Converged — every node receives every delta a retry marker or a durable pending record owes it, under eventual connectivity",
        axis: Tla,
        anchor: "crates/database/db (merge) merge handler; crates/engine/blockstore; crates/database/p2p/src/sync/coordinator/event_handler/mod.rs handle_peer_connected; crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass",
        model_ref: "MC_Conv_{Eventual,RestartEviction,NoHeadRediscovery}.cfg",
        tiers: &[Boundary],
        bound_by: "proofs/tests/behavioral/replication.rs two_node_replication_converges; partition.rs convergence_partition_both_directions_merge, convergence_concurrent_same_doc_writes_merge",
    },
    Property {
        // Only sender retry markers and receiver durable pending records
        // re-drive a head. A head nobody holds an obligation for (a gossip
        // drop, a publish while offline, a head acknowledged then dropped)
        // comes back only through an explicit sync_documents or
        // BranchableSync, or a replicator re-set: there is no on-connect
        // anti-entropy.
        family: "DAG convergence (partition / eviction / restart)",
        name: "INV_Converged assumes fair head rediscovery, which the embedder owns",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/event_handler/mod.rs handle_peer_connected; crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass",
        model_ref: "MC_Conv_NoHeadRediscovery.cfg (RED)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "CRDT merge laws",
        // Generic CrdtField core (DefraConvergence.CrdtField): comm+assoc => the merge
        // fold is order-independent (every field). Idempotence is the dividing line:
        // LWW (lwwMerge) is idempotent => a join-semilattice, re-delivery-safe, no dedup
        // (lww_dup_safe); the counter (Int +, including PNCounter negative deltas)
        // is NOT idempotent (counter_not_idempotent) => it must apply each delta
        // exactly once, the algebraic root of the #4935 double-apply. Counter, LWW,
        // and the mixed Counter×LWW product fully instantiate the core (counterCM /
        // lwwCM / mixedCM). PNCounter is the same signed-delta algebra with
        // decrement enabled, covered by singleStore_pncounter_converges and exact
        // live/restart/storm behavioral tests. The mixed product's cross-field
        // materialization hazard is checked by MixedFieldMaterialization.
        name: "CrdtField: comm+assoc => order-independent; PNCounter refines to signed Counter; mixed Counter×LWW inherits dedup",
        axis: Lean,
        anchor: "crates/building-blocks/crdt/src/field/lww.rs LwwField::merge; crates/building-blocks/crdt/src/field/counter.rs CounterField::merge; crates/building-blocks/crdt/src/stored.rs StoredDocument::apply componentwise field merge",
        model_ref: "DefraConvergence.MixedField (lake build); MC_MixedFieldMaterialization_Green.cfg",
        tiers: &[Contract, Behavioral],
        bound_by: "proofs/tests/lean_conformance.rs lean_merge_result_vocab_matches_rust; proofs/tests/behavioral/partition.rs convergence_concurrent_mixed_lww_and_counter_fields_merge, convergence_concurrent_pncounter_signed_deltas_sum",
    },
    Property {
        family: "CRDT merge laws",
        name: "PCounterGuard: the positive counter's op accepts only non-negative values, so its value never decreases under any run of accepted or refused writes whose sum stays inside i64 (an i64 sum wraps, as in Go)",
        axis: Lean,
        anchor: "crates/building-blocks/crdt/src/field/counter.rs CounterField::op; crates/building-blocks/crdt/tests/field_contract.rs only_the_positive_counter_refuses_a_negative_value",
        model_ref: "DefraConvergence.PCounterGuard.run_ge (lake build)",
        tiers: &[Contract],
        bound_by: "crates/building-blocks/crdt/tests/field_contract.rs only_the_positive_counter_refuses_a_negative_value",
    },
    Property {
        // Two behavioral legs: backfill (a write that pre-dates the replicator is
        // delivered) and full resume across a real disconnect (node1 restarted —
        // its state survives on disk and a post-disconnect write arrives after
        // reconnect). The resume leg needs an actual partition: config-level
        // replicator delete does not gate sync between already-connected peers, so
        // it restarts node1 via the harness `restart_node` (fixed upstream in
        // backbone 025d396 to respawn the configured binary, not the debug path).
        family: "Replicator lifecycle (no-loss / resume)",
        name: "INV_NoLoss — reconnect recomputes the target gap, no block dropped",
        axis: Tla,
        anchor: "crates/database/p2p/src/replicator.rs; crates/database/db/src/merge/push_docs.rs push_existing_docs, retry_doc",
        model_ref: "MC_Replicator_Resumable_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/replicator_lifecycle.rs replicator_backfill_no_loss, replicator_resume_across_restart",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_ObligationConservation / INV_PendingServiceable / INV_PendingHasAuthenticatedProvider / INV_SingleFlight / INV_SingleMergeWriter / INV_SenderMarkersOnly",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync coordinator + pending DAG clock; crates/database/db (merge) push replay; crates/engine/storage peerstore retry markers",
        model_ref: "MC_SyncOwnership_Green.cfg",
        // The deterministic A/B and restart/fanout witnesses live in the
        // integration-test crate, outside this conformance binary's behavioral
        // harness. Keep this binding honest until that harness drives them.
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A transactional write's markers are registered after its commit;
        // the commit returns only once that registration ran, and P2P stops
        // only after the writes the node admitted drained.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "ACT_AckedWriteOwned / INV_ConservedWhileSenderUp — a commit call returns only once its head is owed; while the sender runs every committed head is owed",
        axis: Tla,
        anchor: "crates/database/db/src/merge/txn_broadcaster.rs SyncTxnBroadcaster::submit_update; crates/database/db/src/event/emission.rs register_update_event_callback; crates/entry-points/defra-node/src/lib.rs Node::shutdown",
        model_ref: "MC_SyncOwnership_{Green_MarkerHandoff,Red_UndrainedMarker}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — document scope",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync coordinator + pending DAG clock; crates/database/db (merge) document marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Doc.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — collection scope",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync coordinator + pending DAG clock; crates/database/db (merge) collection marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Collection.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_FetchHasQualifiedProvider / LIVE_EventualCurrency — immutable origin plus durable same-root alternate",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs; crates/database/p2p/src/sync/pending_store/mod.rs; crates/database/p2p/src/sync/coordinator/mod.rs",
        model_ref: "MC_SyncOwnership_Green_Liveness_IrohRelay.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A clock-dispatched fetch gives its slot back after one attempt, its
        // root is due again a backoff after that attempt ends, and an
        // acknowledged head never expires, so after a restart catch-up roots
        // cannot hold every slot while acknowledged heads wait or expire. The
        // model claims only due roots (next_retry_at passed), and a productive
        // attempt gives its slot back at its quantum. Fairness is the
        // scheduler's only: no root has dispatch fairness of its own. The
        // liveness leg assumes a fetch that can complete eventually does
        // (strong fairness on some WComplete): reachability alone does not
        // exclude a provider that answers busy for ever. Dead and Trickle
        // providers' heads are kept but promised no merge, and do not block
        // the others. The clock takes a root's slot together with a fetch
        // permit of its source peer, so no slot is held by a fetch waiting for
        // one while another peer's root could use it; a root whose peer has no
        // permit free, its own fetches' or a pull fetch's, keeps its place.
        // The liveness leg also assumes that pull fetches, which answer this
        // node's own sync requests, at some point stop starting.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "ACT_AttemptsStartInDueOrder / ACT_EndedAttemptQueuesBehind / ACT_AckedRootLeavesOwned / INV_NoSlotWaitsForPermit / LIVE_AckedRootMerged (assumes an eventually serving provider)",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag, FetchTaskEnd; crates/database/p2p/src/sync/coordinator/dag_retry.rs CLOCK_ATTEMPT_QUANTUM; crates/database/p2p/src/sync/replication/handlers.rs handle_dag_needs_fetch; crates/database/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::finish_clock_attempt; crates/database/p2p/src/sync/manager/process/pending_dag.rs try_claim_pending_dag_dispatch, PendingDagLease::finish_attempt, due_pending_dag_retries; crates/database/p2p/src/sync/manager/pending.rs PendingDag::is_expired; crates/database/p2p/src/sync/manager/config.rs SyncConfig::pending_dag_ttl; crates/database/p2p/src/sync/manager/pending.rs PENDING_DAG_TTL; crates/database/p2p/src/sync/coordinator/mod.rs dispatch_due_pending_dag_fetches, DagFetchLimiter::try_reserve_peer, DagFetchLimiter::acquire",
        model_ref: "MC_SyncOwnership_{Green_Dispatch,Green_Dispatch_DeadProvider,Green_Dispatch_Trickle,Green_Dispatch_PeerPermits,Green_Dispatch_PullPermit,Red_HeldSlot,Red_ClaimTimeRearm,Red_AckedTtl,Red_UnboundedProductive,Red_SlotBeforePermit}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The clock hands its dispatches to the merge loop on a lane of their
        // own, one per fetch slot, which the merge loop empties before its
        // channel of merge work: senders waiting for room in that channel
        // are owed every cell it frees, and would otherwise starve the clock.
        // The lane goes first for at most a slot's worth of dispatches in a
        // row, so a finished fetch's DagReady, which it reports through the
        // channel under its slot, still gets its turn. A transport handler,
        // or the merge loop, that completes a root never waits for room; it
        // makes the root due for the clock instead. The storm of merge work
        // is never forced and never made to stop.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_AckedRootMerged under a full merge-loop channel / INV_AdmissionNeverWaitsOnMergeLoop / INV_MergeLoopNeverWaitsOnItself",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/event_lanes.rs DispatchLane, SyncEventReceiver::recv; crates/database/p2p/src/sync/coordinator/mod.rs dispatch_due_pending_dag_fetches; crates/database/p2p/src/sync/manager/process/pending_dag.rs retry_pending_dag",
        model_ref: "MC_SyncOwnership_{Green_Dispatch_Storm,Green_Dispatch_LaneTurns,Red_MergeFifoDispatch,Red_BlockingHandlerSend,Red_LoopWaitsOnItself,Red_LaneAlwaysFirst}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The durable store can hold more acknowledged roots than the
        // in-memory map. A root that had its dispatches gives its entry up
        // while a durable root waits (its record stays), and the resync sweep
        // resumes after the place it admitted at last and gives a peer's
        // places to its roots in turn, so never-serving roots restored first
        // cannot hold the map, or a peer's quota of it, for good, and a peer
        // held at its quota does not hold the others back. A local pull-sync
        // continuation ends at its first turn that does not yield; it is
        // never an acknowledged obligation and is promised no merge.
        // Pull-originated roots have no durable record: a merge that fails, or
        // a yield whose continuation is refused at map or per-peer capacity
        // (reported Failed), is dropped, and its blocks stay unmerged until the
        // next pull or push of the head. Callers own pull completion, and
        // nothing re-drives these at startup.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_DurableRootMerged / LIVE_ContinuationEnds / INV_ContinuationsNeverAcked / INV_MemoryBounded",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pending_dag.rs PendingDagLease::finish_attempt, records_after_resync_cursor, resync_persisted_pending_dags_inner, try_insert_pending_dag, max_pending_dags_per_peer, continue_merge, consume_pending_merge_continuation, MergeTurn; crates/database/p2p/src/sync/manager/pending.rs PendingOrigin, PENDING_ROTATE_AFTER_DISPATCHES; crates/database/p2p/src/sync/replication/handlers.rs handle_block_received, process_merge_batch",
        model_ref: "MC_SyncOwnership_{Green_Memory,Green_Memory_Quota,Red_RetainedOverflow,Red_KeyOrderResync,Red_QuotaPassOver,Red_ImmortalContinuation}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A hub that merges an upstream head forwards it to its own
        // replicators before the merge discharges the upstream pending
        // record; a forward that fails leaves the root unmarked, and any
        // terminal step for a root the database holds and the hub has not
        // marked forwards it, in the same process or after a restart. The
        // witnesses are the p2p crate's forward_tests and the db crate's
        // restart re-drive tests, outside this conformance binary's harness.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_ForwardObligationOwned / LIVE_EventualDownstreamCurrency: the downstream marker is durable before the upstream discharge",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/replication/handlers.rs forward_merged, forward_owed, handle_block_received, process_merge_batch; crates/foundation/defra-core/src/merge.rs MergeOutcome::skip_reason_is_applied; crates/database/p2p/src/sync/replication/forward_tests.rs; crates/database/db/tests/merge/head_publication.rs",
        model_ref: "MC_SyncOwnership_{Green,Green_Liveness_Doc,Red_DischargeBeforeForward,Red_OwedInMemory}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The pusher's outbound head hints wait in one bounded queue drained by
        // a fixed worker pool; one peer holds at most its per-peer cap. A send
        // that times out parks its peer, and peers that have not answered since
        // share one per-peer cap, so any number of dead replicators leave
        // workers for the rest. A peer's queued jobs move to the retry ledger
        // only when a probe (a job started while it was unreachable) times out.
        // Fairness is the scheduler's only; every running send ends by a reply
        // or its send timeout.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_QueueBounded / INV_PermitConservation / INV_PeerLeavesAWorker / INV_UnreachableLeaveAWorker / ACT_DemoteAfterFailedProbe / LIVE_HealthyProgress",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/push_backlog.rs BacklogOwner::enqueue, pop_eligible, park_peer, park_peer_unreachable, peer_answered, retire_idle_cooldowns, take_queued_for_peer; crates/database/p2p/src/sync/coordinator/push_worker.rs spawn_push_workers, run_push_job",
        model_ref: "MC_PushBacklog_{Green,Green_TwoDead,Green_SharedCap,Red_TwoDead,Red_DemoteEveryTimeout,Red_OfflineQueue,Red_NoPeerCap,Red_SpawnPerItem,Red_PermitLeak,Red_RetainHandles}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Admission has two tiers: a head the full map has no entry for is
        // acknowledged on its durable record alone and joins the rotation,
        // instead of being nacked while the store has room, which left the
        // obligation with the sender for as long as the map stayed full.
        // Each peer holds at most its share of the durable records, counted
        // by the store in each registration's own transaction, so one peer's
        // never-serving roots cannot fill the store and leave every other
        // peer nacked. Fairness is per scheduler: each sending peer's retry
        // ledger and the receiver's answers to that peer, never a scope.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_WrittenHeadMerged / INV_DurablePeerQuota: a full map admits on the durable record within each peer's share",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/database/p2p/src/sync/manager/process/pending_dag.rs durable_share_admits, admits_durable_only, max_persisted_pending_per_peer, try_insert_pending_dag; crates/database/p2p/src/sync/pending_store/store.rs take_new",
        model_ref: "MC_SyncOwnership_{Green_Memory,Green_TwoTierQuota,Red_MemoryOnlyAdmission,Red_NoDurablePeerQuota}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A durable record ends only when its root merges or is quarantined:
        // nothing times one out. A root whose provider never serves (gone or
        // asleep for good, its blocks ACP-filtered, a key never granted, a
        // peer whose identity rotated) keeps its record and its place in its
        // peer's share for good, and that peer's later heads are nacked at
        // the share. The share holds one peer's such roots to a quarter of
        // the durable cap, so the roots of four peers still fill the store,
        // after which every new head is nacked at the cap. Every sweep reads
        // every record; one the map has no place for is skipped before its
        // block is read or its DAG walked.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Durable records have no expiry: never-serving roots hold their peer's share for good",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/pending_store/store.rs register, take_new; crates/database/p2p/src/sync/pending_store/shares.rs; crates/database/p2p/src/sync/manager/process/pending_dag.rs restored_root_fits, resync_persisted_pending_dags_inner",
        model_ref: "MC_SyncOwnership_Green_TwoTierQuota.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A success reply means the head is merged or registered, and a nack
        // means it is not registered: the sender parks the hub on a nack. A
        // head registered on its durable record alone is acknowledged on the
        // record the store committed, since the view of the durable records
        // learns of a commit in receipt order, which an earlier commit's hook
        // holds back past the reply.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_SuccessImpliesRegisteredOrMerged / INV_NackImpliesUnregistered / EventuallyAllMerged: the PushLog ack contract",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_pushlog_with_provider_evidence, assert_pushlog_left_durable_state, process_block_inner, Processed; crates/database/p2p/src/sync/pending_store/store.rs register; crates/database/p2p/src/sync/pending_store/view.rs PersistedView::registered",
        model_ref: "MC_PushLogAdmission_{Green,Red_SuccessOnFull,Red_ViewGuard}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The store counts a peer's durable records in the registration's own
        // transaction, so two registrations of the peer that run at once, or
        // that commit while the view of the durable records lags, never both
        // pass its share. A check against the view alone lets them both pass.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_PeerQuota: each peer's share is held in the registration's transaction",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/pending_store/store.rs register, take_new; crates/database/p2p/src/sync/pending_store/shares.rs held, charge, hand_over, give_back; crates/database/p2p/src/sync/manager/process/pending_dag.rs durable_share_admits",
        model_ref: "MC_PushLogAdmission_{Green_PeerShare,Red_ViewShare}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A second PushLog for a CID already being processed joins that
        // processing instead of being nacked: it waits, bounded by a deadline,
        // looks again once the owner lets go, and either acks a block already
        // merged or registered, or takes the guard and processes its own copy.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_SuccessImpliesDurable / INV_OneProcessor / LIVE_EveryJoinEnds: same-CID PushLog joins the in-flight owner",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_pushlog_with_provider_evidence; join.rs join_in_flight, needs_nothing_more; crates/database/p2p/src/sync/queue.rs ProcessQueue::try_acquire",
        model_ref: "MC_PushLogJoin_{Green,Red_NoRecheck,Red_NoGuard,Red_NoDeadline}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A gossip announcement refused for rate, saturation or capacity is
        // written as a durable pending record and promoted into a free slot
        // later; gossip has no retry, so dropping it loses the document.
        // Past a per-peer or total byte budget it is dropped and reported.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_HeardIsOwned / INV_HeardIsOwnedOrReported / INV_NothingDropped / LIVE_AllMerged / LIVE_AllSettled: a refused gossip head is kept durably, or reported past its budget",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/event_dispatcher.rs run_event_dispatcher, kept_when_refused; crates/database/p2p/src/sync/coordinator/event_handler/mod.rs handle_transport_event, handle_transport_event_with_admission; crates/database/p2p/src/sync/coordinator/event_handler/gossip.rs handle_gossip_message, defer_refused_gossip; crates/database/p2p/src/sync/manager/process/gossip_deferral.rs defer_gossip_head, drop_gossip_head; crates/database/p2p/src/sync/pending_store/store.rs register; crates/database/p2p/src/sync/manager/process/pending_dag.rs restored_root_may_fit, resync_persisted_pending_dags; crates/database/p2p/src/sync/coordinator/mod.rs run_pending_dag_resync",
        model_ref: "MC_GossipAdmission_{Green,Green_PastCap,Red_SilentShed,Red_Drop,Red_MemoryDefer,Red_NoPromote,Red_LaneDrop}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A committed write's gossip waits for room in a byte-bounded pending
        // set instead of being shed; only replicators hold a durable marker,
        // so a shed announcement never reaches a gossip-only peer. Its
        // coalescing window holds no publish slot. Past a wait deadline it is
        // dropped and reported.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_CommittedIsOwnedOrReported / INV_PendingBounded / INV_WindowHoldsNoSlot / INV_TasksBounded / LIVE_AllSettled: a committed write's gossip is published, or reported past a wait deadline, on one lane task",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs spawn_non_authoritative_broadcast_task, publish_slot, acquire_non_authoritative_broadcast_slot; crates/database/p2p/src/sync/coordinator/broadcast_room.rs BroadcastRoom::reserve, BroadcastRoom::run, lane; crates/database/p2p/src/sync/coordinator/broadcast.rs broadcast_local_update_with_creator; crates/database/db/src/merge/broadcast_mutator/broadcast.rs spawn_gossip_or_report, broadcast_with_creator",
        model_ref: "MC_SenderBroadcast_{Green,Red_Shed,Red_SilentExpiry,Red_SlotAcrossWindow,Red_UnboundedPending,Red_TaskPerWrite}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "acquire_exclusive / join_acks_only_durable / join_processes_only_unowned: the CID guard is exclusive and a join acks only a safe block",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/queue.rs ProcessQueue::try_acquire, release_sync; crates/database/p2p/src/sync/manager/process/join.rs join_in_flight; crates/database/p2p/src/sync/coordinator/mod.rs DagFetchLimiter::claim_root, RootClaim::drop",
        model_ref: "SingleFlight.Join (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A success reply ends the pusher's retry record, so every acked head
        // must stay reachable from a pending or merged head. A head retires
        // its scope's current head only when it links it as a parent; no head
        // is acked on another head's strength. Witnessed by
        // pushlog.rs sibling_heads_from_one_sender_are_both_retained and
        // sync_manager_tests.rs sibling_heads_from_one_sender_are_both_durable.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_SuccessImpliesRegisteredOrMerged / INV_AckedHeadReachable / EventuallyAllMerged: overflow nacks, and sibling heads of one sender scope each keep their registration",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/head_scope.rs ScopeDecision::of; crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner, assert_pushlog_left_durable_state; crates/database/p2p/src/sync/manager/process/pending_dag.rs try_insert_pending_dag; crates/database/p2p/src/sync/pending_store/store.rs register",
        model_ref: "MC_PushLogAdmission_{Green,Green_Siblings,Red_SuccessOnFull,Red_PriorityCover}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A live ack clears its scope's marker only when the acked head is the
        // scope's whole frontier; with a sibling beside it, the ack is not
        // reported and the retry ladder sends every current head and records
        // them as acked. Witnessed by access_tests.rs
        // live_ack_of_a_sibling_head_does_not_clear_the_scope_marker and
        // db push_docs_common.rs multi_head_doc_reads_acknowledged_after_ladder.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_OneLiveHead / INV_NoStaleRetry / INV_NewestRetained / INV_ClearedScopeDelivered / INV_AckRecordCurrent: latest-head coalescing, a cleared marker never hides an undelivered sibling, and its acked-heads record is the current frontier",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/push_backlog.rs BacklogOwner::enqueue; crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job, is_whole_frontier; crates/engine/storage/src/stores/peerstore.rs refresh_push_failure_with_generation, complete_retry_scope, record_acknowledged_heads",
        model_ref: "MC_PushCoalescing_{Green,Green_Siblings,Red_AppendEvery,Red_StaleRetry,Red_SiblingClear,Red_UnfencedAck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Rejected is assumed content-determined. Under strict (Vera) ACP a
        // signed update the provider refuses is no longer Rejected: it is
        // parked by its document and re-driven (ParkedMerge.tla), so it never
        // reaches the quarantine. An unsigned update's refusal stays
        // Rejected, and is content-determined: no grant can cover it.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_NoSilentDrop / LIVE_SoundEventuallyMerged / LIVE_PoisonQuiesces assume a rejection is content-determined",
        axis: Tla,
        anchor: "crates/database/db/src/merge/acp_merge_handler.rs on_protected_update; crates/database/p2p/src/sync/coordinator/merge_state.rs quarantine_pending_dag",
        model_ref: "MC_PendingDagQuarantine_{Green,Red_OvereagerQuarantine,Red_RetryForever}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // With no pending store installed a success ack is backed only in
        // memory, and a restart loses it. Every production embedding installs
        // a store.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_AckBacked holds only with a pending store installed",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner, extend_registered_root; crates/database/p2p/src/sync/pending_store/mod.rs",
        model_ref: "MC_PendingDagRestart_{Green,Red_ProcessLocal}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Per-field LWW is the DefraDB data model: fields are not atomic across
        // a record, so a converged claim can mix different claimants' fields.
        // Consumers fence on one field (gents: execution_generation).
        family: "Multi-instance claim",
        name: "INV_EventualUnique per field — each AgentRequest field converges to one LWW winner (priority, then bytes)",
        axis: Tla,
        anchor: "crates/building-blocks/crdt/src/field/lww.rs LwwField::merge; gents:crates/gents/src/request/lifecycle/claim.rs claim_inner (foreign substrate, not the defradb.rs binary)",
        model_ref: "MC_Claim_Filtered_Eventual.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Verify-before-merge fires only on the P2P merge path; injecting a
        // forged/unsigned block needs an adversarial peer, not reachable through
        // the public CLI. Rust makes signature verification mandatory (model->code
        // audit) and EUF-CMA is a named crypto boundary, so this is structural.
        family: "Block integrity / signatures",
        name: "INV_OnlyVerifiedMerged — verify-before-merge, author bound to verified DID",
        axis: Tla,
        anchor: "crates/database/db (merge) sig verify; crates/foundation/defra-core/src/batch_signing.rs",
        model_ref: "MC_Integrity_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The unauthorized-denied leg is now observable: a serve-side
        // `tracing::warn!("DEK release DENIED")` at the KMS policy check makes the
        // previously-silent denial loggable, and the behavioral test reads node0's
        // log to confirm node0 denies the DEK to an unauthorized node1's fetch.
        // ECIES secrecy of a released envelope remains a crypto Boundary (proven
        // by crates/database/kms ecies_envelope unit tests).
        family: "KMS key distribution",
        name: "INV_AuthorizedEventuallyGets / no-unauthorized-usable",
        axis: Tla,
        anchor: "crates/database/kms/src/defra_kms.rs serve_request; crates/database/kms/src/nac_dac_policy.rs",
        model_ref: "MC_Kms_Green.cfg",
        tiers: &[Behavioral, Boundary],
        bound_by: "proofs/tests/behavioral/kms.rs kms_unauthorized_dek_release_denied",
    },
    Property {
        family: "KMS key distribution",
        name: "chunks_flatten / chunk_size_le / answers_in_order / answers_length: a key batch split for a transport asks for every key once, no chunk over the per-request limit, and answers every key in its place",
        axis: Lean,
        anchor: "crates/database/kms/src/defra_kms.rs DefraKms::with_inputs, CHUNKS_IN_FLIGHT; crates/building-blocks/inputs/src/service.rs Inner::inline, Inner::answers",
        model_ref: "KmsChunks (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/kms/src/remote_tests.rs a_batch_is_split_to_the_keys_a_transport_carries, a_split_batch_keeps_few_chunks_on_the_wire_at_once, a_retryable_send_failure_of_one_chunk_is_retried",
    },
    Property {
        family: "KMS key distribution",
        name: "WindowBound / PermitBound / RetriedBeforeError / OwnOutcome / AllAnswered: an inline call's chunks run at most flights_per_group at once and max_in_flight on the wire, a chunk errs only after its retries, each chunk's keys get that chunk's own outcome, and every chunk is answered",
        axis: Tla,
        anchor: "crates/building-blocks/inputs/src/service.rs Inner::inline, Inner::round_trip, Inner::answers; crates/database/kms/src/defra_kms.rs DefraKms::with_inputs, CHUNKS_IN_FLIGHT",
        model_ref: "MC_InlineBatchFlights_Green.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/kms/src/remote_tests.rs a_split_batch_keeps_few_chunks_on_the_wire_at_once, a_retryable_send_failure_of_one_chunk_is_retried",
    },
    Property {
        // A merge that needs a DEK fetches it inline on the serial merge
        // executor, blocking unrelated merges for up to RESPONSE_TIMEOUT (5 s,
        // plus up to SUBSCRIBER_WAIT_TIMEOUT, 10 s, when no subscriber is
        // known). KeyUnavailable is retryable, so an unauthorized replica pays
        // this on every re-drive.
        family: "KMS key distribution",
        name: "LIVE_UnrelatedMergeProgress is not claimed: a merge awaiting a DEK holds the merge executor",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/dispatch.rs merge_with_retries; crates/database/p2p/src/kms/pubsub_transport.rs RESPONSE_TIMEOUT, SUBSCRIBER_WAIT_TIMEOUT",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A write asks the KMS once per key scope: every field under document
        // encryption shares the document key, and a field key is never it.
        // Exercised by the testo scenarios
        // encryption::field_commit::*_should_use_dedicated_enc_key_for_individual_fields.
        family: "KMS key distribution",
        name: "DocKeyScopes.same_scope_same_key / different_scope_different_key",
        axis: Lean,
        anchor: "crates/database/db/src/write/prepare.rs KMS key preparation (`generated` memo)",
        model_ref: "DocKeyScopes (lake build)",
        tiers: &[Behavioral],
        bound_by: "tools/integration-test/tests/testo/encryption/field_commit.rs doc_encryption_field_with_doc_and_field_encryption_should_use_dedicated_enc_key_for_individual_fields",
    },
    Property {
        // Encryption reuses PriorityReconcile.lwwCM unchanged: ciphertext and
        // key timing affect when a value can materialize, not which value wins.
        // The behavioral legs preserve encrypted fields selected for filtered
        // replication and receive both encrypted siblings after a real restart,
        // proving identical DAGs and the decrypted winner.
        family: "Encrypted LWW restart/replay",
        name: "INV_NoFilteredLoss / INV_LwwWinner — encrypted delivery remains complete and LWW-convergent",
        axis: Tla,
        anchor: "crates/database/db/src/merge/push_docs.rs; crates/database/p2p/src/sync/coordinator/selective_car_access.rs; crates/building-blocks/crdt/src/field/lww.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/partition.rs convergence_encrypted_lww_restart_merge",
    },
    Property {
        // The model and focused KMS/merge unit tests cover the retry mechanics,
        // but the external harness cannot deterministically hold a DEK request
        // unavailable between ciphertext acknowledgement and receiver restart.
        family: "Encrypted LWW restart/replay",
        name: "INV_AckBacked — acknowledged encrypted replay remains re-drivable across transient KMS failure and restart",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/composite_fields.rs; crates/database/p2p/src/sync/pending_store/mod.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Management-channel auth (NAC gate)",
        name: "INV_OnlyAuthorizedManages — management ops require a valid scoped token",
        axis: Tla,
        anchor: "crates/surfaces/http manage channel; crates/database/db (nac)",
        model_ref: "MC_Auth_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/nac.rs nac_management_requires_admin",
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
        // checked term and blocks admission until the outbox holds it.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NoVoidDecision / INV_RevocationDenies / INV_NothingLost / LIVE_ResubmittedSettles: no decision is served from a voided revision, a revocation confirmed on a void block still denies, nothing a term voided is taken as kept, and what is owed again settles",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/follow/mod.rs Follow::admit, Follow::still, Follow::epoch, reconcile, scan, uncovered; crates/engine/vera/src/vera_rs/follow/journal.rs Journal::record, Journal::begin_replay, Journal::handled, MAX_CONFIRMATIONS, MAX_CONFIRMATION_BYTES; crates/engine/vera/src/vera_rs/follow/lost.rs lost_effects, tainted_by; crates/engine/vera/src/vera_rs/provider/terms.rs Follower::check; crates/engine/vera/src/vera_rs/provider.rs decision_state, admitted_read; crates/engine/vera/src/vera_rs/provider/owed_request.rs locked_worker; crates/engine/acp/src/external/voided.rs OweAgain, VoidedEffect; backbone:acp-light-client HeaderChain::voided",
        model_ref: "MC_FollowDeployment_{Green,Green_Window,Red_KeepsCache,Red_ForgetsRevocation,Red_NoGate,Red_NoTaint,Red_AssumesWindow,Red_NoMark,Red_NoDeliveryGate,Red_CollapsesBatch}.cfg",
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
        // Between asks for a receipt the provider waits for the light
        // client to verify a height above the one it read before it asked
        // and sent, at most a fallback interval. The node serves a receipt
        // from the moment it publishes its block's header, so the receipt
        // is asked for as soon as it can be there; a header verified while
        // the ask or the send was on its way still ends the wait; and a
        // header stream that is down delays the receipt by the fallback,
        // never for good.
        family: "vera.rs receipt wait",
        name: "INV_Prompt / LIVE_Confirms: a receipt is asked for at the verified height that holds it, with no wait on a timer, and is read in the end when no header comes",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/receipt_wait.rs until_receipt, Heads, Fallback, MIN_FALLBACK; crates/engine/vera/src/vera_rs/provider.rs receipt_fallbacks; crates/engine/vera/src/vera_rs/provider/submission.rs resolve_pending; crates/engine/vera/src/vera_rs/follow/chain.rs ProviderChain::send_and_confirm; crates/engine/vera/src/tuning.rs receipt_fallback",
        model_ref: "MC_ReceiptWait_{Green,Red_TimerOnly,Red_SeenAfterSend,Red_Unbounded}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/engine/vera/src/vera_rs/receipt_wait_tests.rs a_receipt_is_asked_for_at_the_next_verified_height_and_not_after_a_sleep, a_header_verified_while_the_request_is_sent_still_ends_the_wait, with_no_header_each_wait_ends_at_its_fallback_and_an_unheard_send_is_sent_again; crates/engine/vera-embedded/tests/receipt_wait.rs writes_are_confirmed_at_their_blocks_header_and_not_at_the_fallback",
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
        anchor: "crates/engine/acp/src/pending.rs PendingRegistration, PendingRegistrations::record, PendingRegistrations::get, PendingRegistrations::clear; crates/engine/acp/src/external/registration.rs settle, settle_with, registrar, recorded_registrar, presenting, note_failure, rewrite, clear; crates/foundation/defra-core/src/request_context.rs RequestContext::bearer_token_for; crates/engine/acp/src/external.rs ExternalDocumentAcp::redrive_owed, ExternalDocumentAcp::check_doc_access, ExternalDocumentAcp::redrive_registration, ExternalDocumentAcp::revoked, ExternalDocumentAcp::transaction_outbox; crates/engine/acp/src/external/settle.rs settle_owed_changes_once, pass, pass_unchecked, settle_change, conclude, submittable, register_first, creators_delegation, settle_owed_before, refusal; crates/engine/acp/src/external/drain.rs recover_marked, send_request, batch, joins_request, settle_applied, MAX_CHANGES_PER_REQUEST; crates/engine/acp/src/outbox/journal.rs OwedJournal, PreparedRequest, RequestOutcome; crates/engine/acp/src/outbox/changes.rs OwedChanges::stage, VALIDATED_WITHIN_OF_NODE_LIMIT; crates/entry-points/defra-node/src/owed_drainer.rs drain, Backoff, PublishRefusals; crates/engine/vera/src/vera_rs/provider/owed_request.rs prepare_owed_request, journal_owed, owed_outcome; crates/engine/vera/src/vera_rs/provider/submission.rs resolve_pending, recover_pending, Note; crates/engine/vera/src/vera_rs/outcome_note.rs OutcomeNote, MAX_NOTED; crates/engine/vera/src/cosmos/dac/owed_journal.rs VeraOwedJournal; crates/engine/acp/src/outbox/staging.rs TransactionOutbox::stage, TransactionOutbox::advise, unchecked; crates/engine/acp/src/external/owed.rs entries, await_transaction, reauthorize, abandon_change; crates/engine/acp/src/outbox/ops.rs stage, settle, mark, mark_in_flight, clear_in_flight, unsettled_validated, reauthorize, discard, acknowledge; crates/engine/acp/src/outbox/record.rs run_epoch, InFlight; crates/engine/acp/src/outbox/deny.rs owed_denial, OwedDenials::on; crates/engine/acp/src/outbox/submission.rs Submission; crates/database/db/src/write/autocommit/helpers.rs register_created_doc, unregister_deleted_doc; crates/database/db/src/write/autocommit/create.rs run_create_attempt; crates/database/db/src/txn/mod.rs DbTxn::owe_acp_registration; crates/engine/acp/src/commit_registrations.rs CommitRegistrations::owe; crates/database/db/src/txn/mod.rs DbTxn::owed_staged, rollback_statement_savepoint; crates/engine/query/src/txn/context.rs AcpRegistrationProjection, denied_by_owed_delete; crates/entry-points/defra-node/src/acp_ops/transactional.rs relationship_in_txn, settle_owed_changes; crates/entry-points/defra-node/src/node_acp.rs gate_external_acp; crates/entry-points/defra-node/src/owed.rs Node::owed_acp_as, Node::await_owed_acp_as, Node::reauthorize_owed_acp_as, Node::abandon_owed_acp_change_as; crates/engine/vera/src/cosmos/dac/owed.rs submit; crates/surfaces/http/src/handlers/acp.rs relationship_operations, add_doc_relationship, remove_doc_relationship; crates/entry-points/defra-node/src/ops/http_ops.rs HttpNodeOps::change_doc_relationship, HttpNodeOps::supports_transactional_relationships; crates/engine/vera/src/cosmos/dac.rs authority",
        model_ref: "MC_AcpOutbox_{Green,Green_Registered,Green_Three,Green_Three_Registered,Green_Live,Green_NoMarker,Green_OwnerChanges,Green_Live_OwnerChanges,Green_Journal,Green_Journal_Registered,Green_Journal_Live,Green_JournalSameBytes,Green_Remote,Green_Remote_HeldKey,Green_Remote_Live,Red_OwedOutsideTxn,Red_NoDenyOverlay,Red_LatestOpOverlay,Red_RefusedDeleteForgotten,Red_OwedAddGrants,Red_WildcardGrant,Red_ChangeBeforeRegistration,Red_OutOfOrder,Red_DrainPastAwaiting,Red_NoInFlightMarker,Red_ReceiptLost,Red_ContestedRegistration,Red_RefusedBlocksDrain,Red_UncheckedDelete,Red_UncheckedRefusedKept,Red_UncheckedHolds,Red_UncheckedPassedKept,Red_TransferWhileOwed,Red_AbandonByAnyone,Red_AbandonInFlight,Red_SilentFailure,Red_SilentDiscard,Red_AmbientRegistration,Red_AmbientRecorded,Red_ReauthorizeUnchecked,Red_JournalNoMarker,Red_JournalNoNote,Red_JournalReceiptLost,Red_BatchNotIsolated,Red_JournalNoFloor,Red_JournalRenewedCredential,Red_RemoteActorFromRequest,Red_RemoteUnchecked,Red_RemoteNoAuthority}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/db/tests/access/external_owed.rs a_registration_that_fails_after_its_commit_is_reported_with_its_reason, a_registration_with_no_authority_waits_for_a_credential_and_is_not_sent, changes_behind_a_parked_registration_go_once_it_is_given_a_credential, a_wait_says_pending_while_the_provider_is_away_and_ends_when_a_record_changes, only_the_actor_that_staged_a_refused_delete_lifts_its_denial_by_abandoning_it, an_abandoned_change_lets_the_changes_behind_it_go_and_only_a_refused_one_is_abandoned, a_refused_change_is_submitted_again_under_a_fresh_credential, a_change_the_provider_never_took_is_reported_as_discarded_and_not_as_settled, a_wait_is_for_a_transaction_of_this_run_and_for_its_callers_entries; crates/entry-points/defra-node/tests/unit/owed_report.rs only_the_actor_that_staged_a_refused_revocation_abandons_it, a_retry_of_an_owed_registration_never_reads_the_token_the_process_holds, a_request_that_carries_no_bearer_registers_nothing_under_a_token_the_process_holds, a_pass_sends_a_registration_with_whichever_owed_delegation_the_report_found; crates/database/db/tests/access/external_drainer.rs a_change_committed_before_a_crash_is_sent_once_after_the_restart, a_change_marked_before_a_crash_is_decided_by_the_note_and_not_sent_again, a_change_marked_before_a_crash_is_decided_by_the_journal_and_not_sent_again, a_change_whose_receipt_was_kept_before_a_crash_is_settled_by_its_hash, an_unavailable_provider_leaves_the_change_marked_until_it_comes_back, a_refused_change_is_parked_and_told_of_and_holds_back_no_other_tuple, the_changes_one_transaction_staged_go_in_one_request, a_refused_request_is_sent_again_one_change_at_a_time, a_change_whose_request_nobody_knows_is_tried_again_without_a_commit, a_marked_change_is_sent_by_a_provider_without_a_journal, transactions_on_a_document_commit_while_the_drainer_settles_there; crates/engine/vera/src/vera_rs/outcome_note.rs an_outcome_of_an_earlier_request_with_the_same_bytes_is_not_the_callers, a_recorded_hash_reads_its_own_outcome_and_no_other; tools/integration-test/tests/verars/owed_note.rs a_command_that_resolves_a_journaled_request_with_its_bytes_notes_the_outcome; crates/database/db/tests/access/external_counts.rs near_the_node_bound_stagers_are_ordered_and_settles_abort_none; crates/entry-points/defra-node/tests/unit/http_staged_relationship.rs a_client_transaction_creates_and_grants_and_the_grant_lands_after_the_commit, a_change_without_authority_is_refused_before_the_commit_and_owes_nothing, a_discarded_or_abandoned_transaction_leaves_nothing_owed, the_acting_identity_comes_from_the_credential_alone, an_identity_token_for_a_did_whose_key_the_node_holds_stages_under_the_node_key; crates/entry-points/defra-node/tests/embedded_vera/http_staged.rs a_client_transaction_that_creates_and_grants_commits_and_the_grantee_reads, a_refused_or_discarded_grant_owes_nothing; crates/entry-points/defra-node/tests/embedded_vera.rs an_idle_node_settles_a_staged_grant_once_its_deployment_is_back, the_grants_one_transaction_stages_land_together, a_restart_settles_a_change_the_chain_could_not_take",
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
        // Peers that merged the same blocks show the same text, a peer's text
        // built one block at a time equals a replay from scratch, every edit
        // lands at its position in its own parent version, and a run typed at
        // one spot is never split by a concurrent one; a write of null reads
        // null exactly while every head writes null. The crdt tests check the
        // same invariants on random DAGs; the db tests merge real blocks
        // between two nodes.
        family: "Text CRDT convergence",
        name: "INV_Converge / INV_MatchesReplay / INV_Intent / INV_PlacedAsIntended / INV_NoInterleave / INV_CriticalReplay / INV_NullConverge / INV_NullMatchesReplay / INV_NullEmpty: Fugue text converges, keeps every edit where its author put it, never interleaves runs, replays a concurrent merge from its critical block, and reads null only when every head writes null",
        axis: Tla,
        anchor: "crates/building-blocks/crdt/src/fugue.rs Tree::apply, Tree::attach, replay, merge_text; crates/building-blocks/crdt/src/field/text.rs TextField (block_data, CausalCrdt diff, block_op, replay, merge_from, check_version, nil_heads); crates/database/db/src/merge/merge_handler/field_causal.rs merge_field_causal, critical_block, causal_history; crates/database/db/src/write/autocommit/helpers.rs apply_field_ops; crates/database/db/src/definition/migration/helpers.rs changes_block_state; crates/building-blocks/crdt/src/fugue_tests.rs, crates/building-blocks/crdt/src/field/testing.rs check_causal, crates/database/db/tests/merge/text_merge.rs",
        model_ref: "MC_TextCrdt_{Green,Red_ArrivalOrder,Red_NoCausalBuffer,Red_CurrentText,Red_NextNode,Red_CommonAncestor,Red_NullLast}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/merge/text_merge.rs concurrent_edits_of_one_text_keep_both_on_every_node; crates/building-blocks/crdt/src/fugue_tests.rs concurrent_inserts_at_one_spot_keep_each_run_whole_in_id_order, an_edit_concurrent_with_a_delete_still_finds_its_spot",
    },
    Property {
        // Fugue's rule puts an inserted character right after its left origin
        // and moves nothing else; a delete hides only its character; two
        // concurrent characters hung in either order give the same tree; a
        // placeholder run kept as one span and cut where an edit lands is the
        // same tree as its chain of characters.
        family: "Text CRDT convergence",
        name: "place_visible / delete_visible / attach_comm / split_run: intention preservation and arrival-order independence of the Fugue tree, and span cuts that change nothing",
        axis: Lean,
        anchor: "crates/building-blocks/crdt/src/fugue.rs Tree::apply, Tree::attach, Tree::traverse, Tree::split",
        model_ref: "TextCrdt.{Fugue,Span} (lake build)",
        tiers: &[Contract],
        bound_by: "crates/building-blocks/crdt/src/fugue_tests.rs a_linear_history_is_its_ops_applied_in_turn, concurrent_inserts_at_one_spot_keep_each_run_whole_in_id_order, concurrent_deletes_of_one_character_remove_it_once",
    },
    Property {
        // The JSON CRDT (#2129). Each path is a register: racing edits resolve
        // by priority, then block ID; an edit wipes only what it had seen below
        // it, so an update beats a concurrent delete; only a set below a path
        // makes it a map, so a delete creates nothing; the conflicts query
        // lists exactly the racers that lost. The defra-core tests check the
        // same invariants on block DAGs; the integration tests merge real
        // blocks between two nodes.
        family: "JSON CRDT per-path merge",
        name: "INV_Converge / INV_MatchesReplay / INV_DisjointKept / INV_SamePathLww / INV_UpdateWins / INV_TypeClash / INV_ConflictsAreLosers / INV_DeleteCreatesNothing / INV_WriteLandsAsRead: per-path JSON merge converges, keeps every edit nothing saw, never lets a delete create a path, keeps two signers' equal edits apart by their nonces, and lands every write, several paths at once included, as its writer reads it",
        axis: Tla,
        anchor: "crates/foundation/defra-core/src/json_crdt.rs Frontier::delta, Frontier::compose, Frontier::materialize, Frontier::conflicts, replay; crates/building-blocks/crdt/src/field/json.rs JsonField (TaggedCrdt: write_data, effect, replay), block_ops, settle_earlier, apply_ops, landed, mismatch; crates/foundation/defra-core/src/json_crdt.rs encode_write, decode_ops; crates/building-blocks/crdt/src/field/tagged.rs local_write, land; crates/database/db/src/merge/merge_handler/field_tagged.rs merge_field_tagged, tagged_past; crates/database/db/src/write/autocommit/helpers.rs prepare_tagged_writes; crates/database/db/src/definition/migration/helpers.rs changes_block_state; crates/foundation/defra-core/src/json_crdt_tests.rs, crates/building-blocks/crdt/tests/json_field.rs, crates/database/db/tests/merge/json_merge.rs, tools/integration-test/tests/legacy/p2p/json_crdt.rs",
        model_ref: "MC_JsonCrdt_{Green_FirstValueOrMap,Green_FirstListOrDelete,Green_MultiPath,Red_DeleteWins,Red_NoCid,Red_DeleteImpliesMap,Red_SharedId,Red_NoAnchor,Red_NoSettle,Red_DeleteMakesParent,Red_MapKeepsBelow}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/merge/json_merge.rs equal_concurrent_json_edits_from_two_signers_converge_in_any_order; crates/building-blocks/crdt/tests/json_field.rs json_meets_the_contract; crates/foundation/defra-core/src/json_crdt_tests.rs disjoint_keys_both_survive_in_any_order, an_update_beats_a_concurrent_delete_above_it",
    },
    Property {
        // A path's register keeps the same winner whatever order edits arrive
        // in, or if one arrives twice; a path's key reads back as the path, and
        // one key starts another only when its path is above the other's; the
        // keys under a path are one stretch of the frontier's sorted entries.
        family: "JSON CRDT per-path merge",
        name: "join_comm / join_assoc / join_idem / decode_encode / encode_prefix / prefix_convex: order-free registers, nesting path keys, and one range per subtree",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/json_crdt.rs beats, encode_path, decode_path, Frontier::under, Frontier::at",
        model_ref: "JsonCrdt.{Lww,Path} (lake build)",
        tiers: &[Contract],
        bound_by: "crates/foundation/defra-core/src/json_crdt_tests.rs diff_then_merge_gives_the_new_value, disjoint_keys_both_survive_in_any_order, one_path_resolves_by_priority_then_block_and_lists_the_loser",
    },
    Property {
        // Custom CRDT types (#2183). A node without a registered type merges
        // the rest of each block, keeps the custom block and a record of it,
        // and acknowledges it; built again with the type, it merges each
        // parked block once, removing its record in the same transaction. A
        // block whose only part was parked still makes the document, and a
        // replay that fails for now (a store error, a key this node does not
        // hold) leaves its block parked. A node storing a value of a type
        // refuses to start without it. The defra-node custom_crdt tests run
        // the same story between real nodes.
        family: "Custom CRDT types",
        name: "INV_NoBlindMerge / INV_MergeOnce / INV_OtherFieldsExact / INV_ConvergeWhenDone / LIVE_Converge: an unregistered type is never merged, a parked block merges exactly once, other fields stay exact, and replicas converge once the type is registered",
        axis: Tla,
        anchor: "crates/building-blocks/crdt/src/field/registry.rs Crdts::delta, Crdts::field, Crdts::part; crates/database/db/src/merge/merge_handler/field.rs merge_field_delta_in_txn; crates/database/db/src/merge/merge_handler/composite_fields.rs process_linked_field_blocks; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document; crates/database/db/src/merge/merge_handler/parked.rs park, replay_parked, replay_in_txn, DB::start_parked_replay; crates/database/db/src/merge/merge_handler/error.rs MergeError::fails_block, MergeError::needs_key; crates/database/db/src/merge/merge_handler/merge_keys.rs with_local_history_keys; crates/database/db/src/database/stored_crdts.rs record, DB::check_stored_crdts; crates/entry-points/defra-node/tests/custom_crdt/replication.rs, crates/database/db/tests/merge/parked_errors.rs",
        model_ref: "MC_UnknownCrdt_{Green,Red_DropDeferral,Red_DoubleReplay,Red_Lww,Red_RemoveStored,Red_NoDocument,Red_FailStall}.cfg",
        tiers: &[Contract],
        bound_by: "crates/entry-points/defra-node/tests/custom_crdt/replication.rs a_node_without_the_type_parks_its_blocks_until_it_has_the_type; crates/database/db/tests/merge/parked_errors.rs a_parked_block_fails_only_for_an_error_of_its_own",
    },
    Property {
        // Every existing type keeps its definition bytes (code, no name), so
        // definition CIDs do not move; a custom type is code 8 plus its name;
        // reading back what a type writes gives that type. The defra-node
        // definition_cids golden test pins the CIDs themselves.
        family: "Custom CRDT types",
        name: "native_unchanged / custom_not_native / decode_encode / encode_injective / old_codes_read_back: custom types leave native definitions byte-identical and round-trip",
        axis: Lean,
        anchor: "crates/foundation/schema/src/ctype.rs CType::to_u8, CType::custom_name, CType::from_definition; crates/foundation/schema/src/cid.rs field_to_delta_with_priority; crates/foundation/defra-core/src/block_delta.rs FieldDefinitionDeltaPayload; crates/entry-points/defra-node/tests/definition_cids.rs",
        model_ref: "CustomCrdtCode (lake build)",
        tiers: &[Contract],
        bound_by: "crates/entry-points/defra-node/tests/definition_cids.rs native_definitions_keep_their_ids",
    },
    Property {
        // A write pushes each document's heads to every replicator that should
        // get the document, and a `@branchable` collection's heads only to
        // unfiltered replicators. The acknowledgement read compares at the
        // same scope, through the same AckScope rule, so a filtered
        // replicator's acknowledgements are read per document.
        family: "B3 filtered replication",
        name: "INV_AckedReadsAcked / INV_AckedIsHeld / INV_NeverPushedNeverAcked / INV_FilterRespected: replication acknowledgements are read at the scope they are pushed at",
        axis: Tla,
        anchor: "crates/database/p2p/src/ack_scope.rs AckScope::new; crates/database/p2p/src/replicator.rs ReplicatorInfo::ack_scope; crates/database/p2p/src/sync/coordinator/broadcast.rs SyncCoordinator::push_jobs; crates/database/db/src/merge/replication_ack.rs replication_acknowledged",
        model_ref: "MC_ReplicationAckScope_{Green,Red_MainRule,Red_PushToFiltered}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A replicator added with no collections is stored, listed, and
        // restored on start, as in Go, and receives no pushes until it has a
        // collection (#2702).
        family: "B3 filtered replication",
        name: "INV_ListedAsAdded / INV_NoPushOutsideCollections: a replicator with no collections is listed and pushed nothing",
        axis: Tla,
        anchor: "crates/database/p2p/src/bitswap/registry.rs store_info, ReplicatorRegistry::load_from_infos, remove_peer_collections; crates/database/p2p/src/sync/coordinator/broadcast.rs replicator_in_collection",
        model_ref: "MC_ReplicatorEmptyCollections_{Green,Red_DropOnStore,Red_DropOnLoad,Red_EmptyMeansAll}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Storing or loading a replicator record always leaves its entry, the
        // one with no collections included; an empty entry grants nothing and
        // is picked for no collection's pushes.
        family: "B3 filtered replication",
        name: "ReplicatorEmptyCollections.stored_is_listed / restart_lists_every_record / empty_grants_nothing / empty_gets_no_push / push_only_to_members",
        axis: Lean,
        anchor: "crates/database/p2p/src/bitswap/registry.rs store_info, ReplicatorRegistry::load_from_infos, ReplicatorRegistry::is_any_replicator; crates/database/p2p/src/sync/coordinator/broadcast.rs replicator_in_collection",
        model_ref: "ReplicatorEmptyCollections (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A transaction ID travels in the request context. Only the caller
        // that opened the transaction runs in it, a read-only transaction
        // refuses writes, a finished transaction runs nothing, a discard
        // leaves nothing in the store, and a commit that clashes with a
        // newer commit fails.
        family: "Explicit transactions in the request context",
        name: "INV_OnlyOwnerRuns / INV_ReadOnlyNeverWrites / INV_FinishedNeverRuns / INV_DiscardLeavesNothing / INV_NoLostUpdate",
        axis: Tla,
        anchor: "crates/database/db/src/txn/registry/lifecycle.rs begin_as, run_as, commit_as, discard_as; crates/database/db/src/txn/owner/mod.rs Owners::run, Owners::finish; crates/entry-points/defra-node/src/ops/request.rs txn_begin, txn_commit, txn_discard; crates/engine/query/src/runner/turn.rs read-only rejection; tools/integration-test/tests/testo/txn/txn_context.rs",
        model_ref: "MC_TxnContext_{Green,Red_NoOwnerCheck,Red_ReadOnlyWrites,Red_RunAfterEnd,Red_DiscardLeaks,Red_NoConflictCheck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Collection patch, set-active, set-migration, add/list lens and get
        // collections each have one core function every edge calls: it
        // checks NAC before any effect, runs in the transaction the context
        // carries or else autocommits, and refuses a patch inside one.
        family: "Schema-evolution operations in the core",
        name: "INV_NoEffectWithoutPermission / INV_TxnNeverAutocommits / INV_PatchInTxnChangesNothing",
        axis: Tla,
        anchor: "crates/database/db/src/txn/registry/lifecycle.rs get_collections_as, patch_collection_as, set_active_collection_version_as, set_migration_as, add_lens_as, list_lenses_as; crates/entry-points/defra-node/src/ops/collection.rs; crates/surfaces/http/src/handlers/{collections,lens}.rs",
        model_ref: "MC_CoreDispatch_{Green,Red_NoNac,Red_IgnoreTxn}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Listing lenses returns every module of every lens, in order, as
        // Go's `model.Lens`; it used to keep only the first.
        family: "Schema-evolution operations in the core",
        name: "LensListing.listed_keeps_modules / listed_length",
        axis: Lean,
        anchor: "crates/foundation/lens/src/wasm.rs WasmTransformStore::list; crates/foundation/lens/src/store.rs list; crates/database/db/src/txn/registry/lifecycle.rs list_lenses_as",
        model_ref: "LensListing (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // #2378: a lens's ID is the CID of blocks that record every module and
        // every argument value, as Go's lens store writes them; it used to
        // hash the first module and only string arguments.
        family: "Schema-evolution operations in the core",
        name: "LensId.configBlock_injective / new_numbers_differ / new_keeps_later_modules / empty_lens_has_block",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/lens_block.rs build_lens_ipld_blocks, build_module_blocks; crates/database/db/src/definition/migration/committed_lenses.rs PreparedLens::new, module_arguments; crates/foundation/defra-core/src/lens_argument.rs encode_lens_argument",
        model_ref: "LensId (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/foundation/defra-core/tests/lens_block_id.rs every_module_and_argument_value_counts, an_empty_lens_has_gos_config_block_cid",
    },
    Property {
        // #2517: a document saved at another version is stored with that
        // version's layout only when it is a real definition of the same
        // collection; a placeholder (often in the orphan collection) is
        // treated as unknown, so the value stays under the owning collection.
        family: "Schema-evolution operations in the core",
        name: "DocStoreTarget.stored_under_owner / placeholder_never_lays_out / old_stores_under_orphan",
        axis: Lean,
        anchor: "crates/database/db/src/collection/crud.rs Collection::save_with_datastore_priorities",
        model_ref: "DocStoreTarget (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // #2379: an index answers queries only while no reindex is owed;
        // schema changes and merges that store a document off the active
        // version across a lens owe a fresh record, and a rebuild clears only
        // the record it started from. Main before the fix refused every index
        // once a lens existed (Red_LensGate). A query's index scan reads the
        // owed record again in its own transaction and re-plans to a
        // collection scan when one stands, so a merge that commits between
        // the plan and the scan cannot make it miss a document (Red_TrustPlan).
        family: "Schema-evolution operations in the core",
        name: "INV_IndexMatchesReads / INV_SettledIndexAnswers / INV_ScanMatchesReads",
        axis: Tla,
        anchor: "crates/database/db/src/definition/migration/reindex/owed.rs owe_reindex, clear_owed_reindex; crates/database/db/src/collection/mod.rs queryable, Collection::validate_write_definition_read; crates/database/db/src/merge/merge_handler/stale_version.rs DbMergeHandler::owe_reindex_off_active_version; crates/engine/query/src/planner/builder/index_methods.rs try_select_index; crates/database/db/src/read/lensed/autocommit/scan.rs get_by_index_scan_inner; crates/engine/query/src/runner/query/select.rs execute_select_internal",
        model_ref: "MC_LensIndexAnswer_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/merge/plan_scan_race.rs",
    },
    Property {
        // An explicit transaction's ID is `sequence * workers + worker`. The
        // owner is the ID modulo the worker count and the sequence is the
        // quotient, so the round trip is exact for every worker below the
        // count, the owner is always a real worker, and two different
        // (sequence, worker) pairs never share an ID.
        family: "Explicit transactions in the request context",
        name: "TxnId.owner_encode / sequence_encode / owner_lt / encode_injective / decode_encode",
        axis: Lean,
        anchor: "crates/database/db/src/txn/owner/id.rs TxnId::new, TxnId::owner; crates/database/db/src/txn/owner/tests.rs",
        model_ref: "TxnId (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Storage transaction validation",
        name: "Validated point reads reject crossing writes",
        axis: Tla,
        anchor: "crates/engine/storage/src/backends/regolith/transaction.rs; proofs/tests/behavioral/ssi.rs",
        model_ref: "regolith@e844baefe838f3af76403afd46c4b42f1a0b98a5 proofs/tla/DefraLevel.tla",
        tiers: &[Behavioral, Boundary],
        bound_by: "proofs/tests/behavioral/ssi.rs ssi_write_skew_prevented",
    },
    Property {
        // The replay-capability gate lives inside the P2P replication wire
        // protocol; presenting a forged/expired capability needs a custom peer,
        // not the public CLI. Rust-only hardening Go lacks; structural.
        family: "P2P explicit-replay capability gate",
        name: "INV_OnlyLegitAccepted — forged/expired/wrong-target capability rejected",
        axis: Tla,
        anchor: "crates/database/p2p capability replay gate",
        model_ref: "MC_Capability_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The pending entry, its durable record across a restart, and the
        // merge (the collection/creator check and the strict-ACP bypass) reuse
        // the admission verdict without re-checking expiry or revocation.
        // Only KMS key release verifies again. Revocations live in process
        // memory and are lost on restart.
        family: "P2P explicit-replay capability gate",
        name: "INV_TtlCapped / INV_RevokedNeverAccepted hold at PushLog admission only",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/authorization.rs validate_explicit_replay_authorization; crates/database/p2p/src/explicit_replay.rs",
        model_ref: "MC_Capability_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "NAC lifecycle privilege-escalation",
        name: "INV_NoPrivEsc — no escalation across enable/disable/restart",
        axis: Tla,
        anchor: "crates/database/db (nac) lifecycle",
        model_ref: "MC_Nac_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/nac_lifecycle.rs nac_lifecycle_no_escalation",
    },
    Property {
        // Enabling sets NAC up only the first time: neither starting the node
        // with the enable option nor an enable call turns a temporarily
        // disabled NAC back on, so only an authorized re-enable does, as in Go.
        family: "NAC lifecycle privilege-escalation",
        name: "INV_ReEnableNeedsPersistedAdmin through enable and startup: a disabled NAC stays disabled until an authorized re-enable",
        axis: Tla,
        anchor: "crates/engine/acp/src/nac/node_acp/lifecycle.rs NodeACP::enable; crates/database/db/src/nac/mod.rs NacManager::initialize",
        model_ref: "MC_Nac_{Green,Red_EnableReEnables,Red_EnableAddsOwner,Red_StartReEnables}.cfg",
        tiers: &[Contract],
        bound_by: "crates/engine/acp/tests/nac_tests.rs enabling_a_disabled_nac_is_refused; crates/database/db/tests/nac/tests.rs a_disabled_nac_stays_disabled_when_restarted_with_the_enable_option, enabling_a_disabled_nac_is_refused; crates/entry-points/defra-node/tests/unit/http_acp.rs a_stranger_cannot_enable_a_disabled_nac_over_http",
    },
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
    Property {
        // Every open store exports its engine's commit and policy tickers. A
        // read of the totals, or the store's close, adds what the engine
        // counted past the export's mark by swapping the mark forward with one
        // compare-and-swap; only the winner adds, exactly the stretch it
        // claimed. The close adds before it leaves the map. The finding
        // configuration shows the claim and the add are two steps: a read can
        // briefly miss a closed store's counts that another read claimed and
        // has not added yet. The race test exercises the code on real threads;
        // nothing drives this model against the binary.
        family: "Engine ticker export",
        name: "INV_Accounted / INV_NeverOvercount / INV_ClosedComplete / INV_LeftClaimed / INV_QuiescentExact / PROP_TotalNeverBack / PROP_MarkNeverBack / LIVE_EveryCallEnds: every count reaches the exported total once, and the total never goes back",
        axis: Tla,
        anchor: "crates/building-blocks/telemetry/src/engine_tickers.rs Source::publish, EngineTickerExport::drop, engine_ticker_totals; crates/engine/storage/src/backends/shared.rs TransactionMetrics",
        model_ref: "MC_EngineTickers_{Green,Green_TwoSources,Red_Backward,Red_BlindStore,Red_GiveUp,Red_StaleSeen,Red_RemoveFirst,Red_WeakClose,Red_Reset,Red_SharedStats,Red_CountAfterClose,Finding_ClaimGap}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Engine ticker export",
        name: "failed_attempt_changes_nothing / run_gain / run_total_eq_gains / publishAlone_comm / closed_export_adds_exactly: a mark moved by compare-and-swap adds each count once",
        axis: Lean,
        anchor: "crates/building-blocks/telemetry/src/engine_tickers.rs Source::publish",
        model_ref: "EngineTickers.Mark, EngineTickers.Race (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A placed pool claims its CPUs in the process's ledger, one counter
        // per CPU: alone, moving each counter from 0 to 1 with one
        // compare-and-swap and giving back what it took when one CPU is
        // held; or shared, adding one whatever the counter was. After its
        // capped claims alone fail it shares and says so. Its Hold gives back
        // exactly what it raised once the pool and every thread placed
        // through it are gone. A network runtime sized from the machine
        // claims whole cores the same way, alone only, and gives up holding
        // nothing instead of sharing.
        family: "Worker pool CPU ledger",
        name: "INV_AloneTakesDisjoint / INV_AloneRunsAlone / INV_TakenAlone / INV_AllOrNothing / INV_HeldNeverFree / INV_CountExact / INV_DoneLeavesNothing / LIVE_StartEnds: placed pools never share a CPU unless one of them says it shares",
        axis: Tla,
        anchor: "crates/foundation/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop; crates/foundation/workers/src/placement/reserve.rs reserve, shared; crates/foundation/workers/src/placement/edge.rs EdgeCores::reserve_on, EdgeCores::reserve; crates/foundation/workers/src/pool/placer.rs Placer::holding; crates/foundation/workers/src/pool/start.rs Pool::start_in; crates/foundation/edge/src/platform/native.rs build",
        model_ref: "MC_CpuLedger_{Green,Green_Edge,Red_SplitTake,Red_NoRollback,Red_ReleaseLoser,Red_ShareUncounted,Red_NoAttemptCap,Red_ReleaseEarly,Red_RunOutsideHold,Red_DoubleRelease,Red_ForgetHold,Red_EdgeNoCap}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Worker pool CPU ledger",
        name: "hold_all_or_nothing / rollback_restores / hold_won_alone / hold_step / share_step / release_alone_step / release_shared_step: a claim alone takes all or nothing, the counters stay exact, Holds claimed alone stay apart",
        axis: Lean,
        anchor: "crates/foundation/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop",
        model_ref: "CpuLedger.{Hold,Count,Exclusive} (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Every signing key lives in one process-wide store, one slot per
        // DID. Holders (FFI identity handles, nodes) take and give back
        // holds; the hold count, the pin and the key change together in one
        // swap, a slot never leaves the map, and a clear only unpins and
        // drops keys nothing holds. While a DID has a key its signer is
        // fixed: a hold with another signer, or a pin over a held DID, is
        // refused, so no holder is re-routed to a host context it did not
        // bring.
        family: "Signing identity store",
        name: "INV_CountExact / INV_KeyWhileHeld / INV_KeyMatchesHolders / INV_OwnSigner / INV_AcceptSame: a DID's slot counts its holds exactly, a held DID always answers its key, no key outlives its holders unless pinned, no holder signs through a signer it did not bring, and a hold with the held host context is never refused",
        axis: Tla,
        anchor: "crates/foundation/defra-core/src/signing/identity_store.rs hold_identity, retain_identity, release_identity, store_identity, clear_identity_store, same_signer, IdentityHold; crates/foundation/defra-core/src/signing.rs RemoteSigner::host_context; crates/entry-points/ffi/src/remote_signer/mod.rs FfiRemoteSigner::host_context; crates/entry-points/ffi/src/remote_signer/register.rs store",
        model_ref: "MC_IdentityStore_{Green,Red_SplitSwap,Red_DropCount,Red_ClearRemoves,Red_ClearDropsHeld,Red_RetainAbsent,Red_ReplaceOnHold,Red_PinReplacesHeld,Red_RefuseSameContext}.cfg",
        tiers: &[Contract],
        bound_by: "crates/foundation/defra-core/src/signing/identity_store_tests.rs the_last_hold_on_an_identity_removes_its_key, a_retained_identity_outlives_the_hold_it_was_taken_from, a_pinned_identity_survives_its_holds, a_clear_keeps_held_keys_and_their_count, a_held_did_refuses_another_signer, a_pin_replaces_only_an_unheld_signer, a_held_did_takes_a_signer_with_the_same_host_context; crates/entry-points/ffi/src/remote_signer/registration_tests.rs a_second_registration_of_a_held_did_is_refused_until_the_first_is_freed, a_second_registration_with_the_same_context_shares_the_signer",
    },
    Property {
        family: "Signing identity store",
        name: "empty_keeps / hold_keeps / hold_keeps_signer / hold_accepts_same / hold_refuses_other / retain_keeps / retain_answers_key / release_keeps / pin_keeps / pin_keeps_held_signer / clear_keeps / held_has_key / free_has_no_key: every operation keeps the count exact and the key present exactly while pinned or held, a held DID's signer never changes, and a hold with its own signer is taken",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/signing/identity_store.rs Holders, same_signer, hold_identity, retain_identity, release_identity, store_identity, clear_identity_store",
        model_ref: "IdentityStore (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A peer key the caller hands in for one start (Go's P2PPrivateKey)
        // is used as given: never checked against the store and never
        // recorded in it, so a later start without it carries on with the
        // store's own key.
        family: "Peer key loading",
        name: "INV_StartKeyStarts / INV_StartKeyLeavesNoMark / INV_PlainKeyStable: a start with a start key always starts, leaves no mark that refuses a later plain start, and never changes the key plain starts use",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/peer_key.rs load_with_keyring, load_or_create; crates/entry-points/defra-node/src/start_peer_key.rs StartPeerKey; crates/entry-points/defra-node/src/libp2p_runtime.rs assemble_libp2p; crates/entry-points/defra-node/src/p2p_runtime.rs setup_iroh; crates/entry-points/ffi/src/node/mod.rs build_node_state",
        model_ref: "MC_PeerKeyLoad_{Green,Red_StartKeyChecked,Red_StartKeyRecorded}.cfg",
        tiers: &[Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/peer_keyring.rs a_start_key_is_used_as_given_and_leaves_a_plain_store_as_it_was, a_start_key_starts_a_keyring_store_without_changing_it",
    },
    Property {
        family: "Peer key loading",
        name: "start_key_is_used / start_key_leaves_store / plain_after_start_key: a start key is answered as given, the store comes back unchanged, and a plain start after it loads what it would have without it",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/peer_key.rs load_with_keyring, load_or_create",
        model_ref: "PeerKeyLoad (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Calls a C caller makes on one explicit transaction run one at a
        // time, in call order, even when a call returns before its work runs
        // (the asynchronous query): each takes a ticket as it names the
        // transaction, and a ticket dropped before its turn is skipped once.
        family: "FFI transaction turns",
        name: "INV_OneRunning / INV_NoOvertake / Progress: calls on a transaction run one at a time, none overtakes an earlier ticket (a commit never lands before work asked for first), and no call waits forever on a dropped ticket",
        axis: Tla,
        anchor: "crates/entry-points/ffi/src/state/turns.rs Turns::take, Turns::finish, Ticket::wait, Ticket; crates/entry-points/ffi/src/target.rs resolve_target, TxnCall; crates/entry-points/ffi/src/request.rs Request::resolve; crates/entry-points/ffi/src/query/exec.rs Prepared::run, run_in_turn, defra_query_execute_async; crates/entry-points/ffi/src/txn/lifecycle.rs defra_transaction_commit, defra_transaction_discard",
        model_ref: "MC_TxnTurns_{Green,Red_NoRecheck,Red_CancelSkipsNow}.cfg",
        tiers: &[Contract],
        bound_by: "crates/entry-points/ffi/src/state/turns_tests.rs tickets_are_served_in_the_order_they_were_taken, a_ticket_dropped_before_its_turn_is_skipped, a_ticket_dropped_on_its_turn_serves_the_next, calls_from_several_threads_never_run_at_once; crates/entry-points/ffi/src/call/tests.rs an_async_query_in_a_transaction_returns_before_it_runs, calls_on_a_transaction_run_in_the_order_they_were_made",
    },
    Property {
        family: "FFI transaction turns",
        name: "take_hands_out_next / take_moves_next / skip_passes_only_dropped: tickets come out distinct and in call order, and skipping passes only dropped tickets",
        axis: Lean,
        anchor: "crates/entry-points/ffi/src/state/turns.rs Turns::take, Turns::finish",
        model_ref: "TxnTurns (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A vector index description's stored form is the normalized one:
        // Go's defaults filled in, and an empty algorithm or metric read as
        // the default. Normalizing is stable, so the stored form never drifts.
        family: "Vector index defaults",
        name: "normalize_idempotent / normalize_hnsw_fills / normalize_other_unchanged / empty_reads_default / nonempty_reads_itself: normalizing twice is normalizing once, an HNSW index is stored with every parameter set, any other index is untouched, and an empty string reads as the default",
        axis: Lean,
        anchor: "crates/foundation/schema/src/vector_index_defaults.rs VectorIndexDescription::normalized, empty_is_default",
        model_ref: "VectorIndexDefaults (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A document call through a selected collection version (possibly
        // inactive) pins that version in the transaction's catalog for its
        // turn: every name lookup sees it, writes are recorded at it, reads
        // through it store nothing back, and the pin ends with the turn.
        family: "Selected collection version",
        name: "INV_PinScoped / INV_UnpinnedSeesActive / INV_ReadNoSideEffect / INV_WriteAtSelected: a pin never outlives its turn, an unpinned turn sees the active version, a read through an inactive version changes nothing, and a write lands at the selected version",
        axis: Tla,
        anchor: "crates/database/db/src/txn/catalog.rs Catalog::pin, Catalog::get; crates/database/db/src/txn/registry/owned.rs OwnedTxn TxnTurn::pin_collection; crates/engine/query/src/runner/turn.rs run_turn_with_delegation, pin_selected_version; crates/database/db/src/collection/provider.rs TxnCollectionProvider::get_collection; crates/database/db/src/txn/mod.rs DbTxn::validate_collection_for_write; crates/database/db/src/read/lensed/fetcher/migration.rs update_datastore; crates/entry-points/ffi/src/document/run.rs DocumentCall",
        model_ref: "MC_VersionPin_{Green,Red_PinLeaks,Red_WriteBackOnPin}.cfg",
        tiers: &[Contract],
        bound_by: "crates/entry-points/ffi/src/document/version_tests.rs a_document_added_through_an_old_version_is_recorded_at_it, a_get_answers_the_fields_of_the_version_it_reads_through, a_selected_version_does_not_outlive_its_call_in_a_transaction",
    },
    Property {
        family: "Selected collection version",
        name: "pinned_name_answers_pin / other_name_answers_cache / cleared_answers_cache: a pin answers only for its own name, and clearing it leaves the catalog answering exactly what the cache holds",
        axis: Lean,
        anchor: "crates/database/db/src/txn/catalog.rs Catalog::get, Catalog::pin, Catalog::unpin",
        model_ref: "CatalogPin (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // `default_pool()` answers the first registered pool that still runs,
        // else one pool started once for the process. Registration moves the
        // slot only off a pool that stopped, with one compare-and-swap; the
        // started pool is built under a one-permit turn and published once; a
        // worker thread never waits for that start; `Pool::stop` never times
        // its own deadline on the pool it stops.
        family: "Worker default pool",
        name: "INV_NeverHandsOutClosing / INV_FirstRunningWins / INV_RegisteredWins / INV_OneStart / INV_OneDefault / INV_ErrorHonest / INV_StopDeadlineElsewhere / INV_MakeDefaultTellsTruth / INV_WorkerNeverWaits / INV_RegistrationKeepsNothingAlive / LIVE_EveryCallReturns / LIVE_EveryRegisterReturns: one default pool, never a closing one",
        axis: Tla,
        anchor: "crates/foundation/workers/src/pool/default.rs default_pool, Pool::make_default, DefaultPool::register, DefaultPool::get, DefaultPool::started; crates/foundation/workers/src/pool/mod.rs Pool::stop; crates/foundation/workers/src/pool/deadline.rs delay_on",
        model_ref: "MC_DefaultPool_{Green,Red_AnswerYes,Red_BlindRegister,Red_FailKeepsTurn,Red_NoClosingCheck,Red_NoRecheck,Red_NoRetry,Red_NoTurn,Red_RacyCell,Red_RegisterClosing,Red_RegisterOverwrites,Red_ShutStarted,Red_StartedFirst,Red_StopAsksFirst,Red_StrongSlot,Red_WorkerRefusedFirst,Red_WorkerWaits}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Worker default pool",
        name: "first_running_wins / stopped_never_selected / later_registrations_change_nothing / first_calls_agree / registered_wins / stop_deadline_elsewhere: the first running registered pool wins, a stopped one never does, every caller gets one started pool",
        axis: Lean,
        anchor: "crates/foundation/workers/src/pool/default.rs DefaultPool::register, DefaultPool::get",
        model_ref: "DefaultPool.{Slot,Cell,Get} (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // `partition::convergence_delete_update_race_preserves_tombstone`
        // partitions a replicated doc, deletes on one side, updates a mutable field
        // on the other, then heals and asserts the merged materialized view stays
        // tombstoned on both replicas. The TLA red leg captures the bad policy:
        // committing an active update from a stale whole-document snapshot clears
        // the delete marker.
        family: "Document materialization status convergence",
        name: "INV_DeletedMarkerAbsorbs — active rematerialization never clears a tombstone",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/composite_persist.rs handle_deletion / persist_merged_document; crates/foundation/defra-core/src/block_delta.rs CompositeDeltaPayload status",
        model_ref: "MC_DocumentMaterialization_Green.cfg; DefraConvergence.DocumentMaterialization.delete_active_age_converge",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/partition.rs convergence_delete_update_race_preserves_tombstone",
    },
    Property {
        // The DID-binding half (a distinct identity cannot impersonate another)
        // IS observed behaviorally by the ACP test (ungranted bob denied). The
        // signer-binding half — forging a malformed / alg-confused token, or one
        // whose iss/signer DID mismatches its signature — is unreachable through
        // the public CLI (the client always signs correctly), so it is Boundary.
        family: "JWT issuer / algorithm binding",
        name: "INV_TokenBindsGenuineDid — alg/iss/sig bound to did(pubkey)",
        axis: Tla,
        anchor: "crates/building-blocks/identity token verification",
        model_ref: "MC_Jwt_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "CID content-addressing determinism + Block canonicalization",
        name: "cid_injective_mod_hash — same normal-form content yields same CID",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/block.rs Block::generate_cid",
        model_ref: "Cid (lake build)",
        tiers: &[Behavioral, Contract],
        bound_by: "proofs/tests/behavioral/cid.rs cid_determinism_same_content_same_docid",
    },
    Property {
        family: "Deferred-ACP overlay consistency",
        name: "INV_FailClosedActive — txn-local ACP projection gates as committed state would",
        axis: Tla,
        anchor: "crates/engine/query/src/txn/context.rs AcpRegistrationProjection::owed_creator, check_doc_access_with_projection, is_doc_registered_with_projection, is_creator; crates/database/db/src/txn/mod.rs DbTxn::acp_registration_projection; crates/engine/acp/src/pending.rs PendingRegistrations::record, PendingRegistrations::get, PendingRegistrations::clear; crates/database/db/src/write/autocommit/helpers.rs register_created_doc, unregister_deleted_doc",
        model_ref: "MC_DeferredAcp_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/deferred_acp.rs deferred_acp_txn_local_gating",
    },
    Property {
        // Lean marker values (mNull=0..mIntMax=253) verified to match the live
        // `crates/engine/storage/src/encoding/mod.rs` constants by inspection, but those
        // are `pub(crate)` (no static assert without widening prod visibility) and
        // a plain ordered query may sort in memory rather than exercise the index
        // key encoding — so neither tier is cleanly realizable. Boundary.
        family: "Order-preserving key encoding",
        name: "asc_strictly_order_preserving_* — encoding is a strict-order embedding",
        axis: Lean,
        anchor: "crates/engine/storage/src/encoding/mod.rs type-marker constants",
        model_ref: "OrderEncoding (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // `sort_in_key_order` is private to the planner, so no contract test
        // reaches it; crates/entry-points/defra-node/tests/in_filter_order.rs runs the
        // queries it orders over an ASC, a DESC and no index. Boundary.
        family: "Order-preserving key encoding",
        name: "lookups_strictly_increasing / reversed_lookups_strictly_decreasing — `_in` point lookups run in key order, each once",
        axis: Lean,
        anchor: "crates/engine/query/src/planner/index_selection/filter_to_scan.rs filter_to_index_scan, sort_in_key_order",
        model_ref: "OrderEncoding.InLookup (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The flat code table is a pure in-memory structure loaded once per
        // graph generation; its laws are proven, and the unit tests in
        // crates/database/db/tests/vector/diskann_resident.rs exercise the Rust side.
        family: "DiskANN resident code table",
        name: "get_returns_inserted / get_in_bounds / insert_refuses_other_length: each slot reads back its own code, in bounds",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/cache.rs Codes::insert, Codes::get",
        model_ref: "DiskAnnCodes (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // No test binds graph.rs walk to WalkCutoff.
        family: "DiskANN walk cutoff",
        name: "cut_step_eq_step — skipping neighbours ranked after a full frontier's last place leaves the frontier the full step would",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/graph.rs walk, each_edge, by_distance",
        model_ref: "WalkCutoff.cut_step_eq_step / distance_only_cutoff_drops_a_tie",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Pure functions over the schema; the Rust side is exercised by
        // crates/engine/query/src/planner/joins/relation_paths_tests.rs and the
        // validate_select tests in crates/engine/query/src/runner/plan_validation.rs.
        family: "Filter relation paths and scalar leaves",
        name: "cut_eq_longest / longest_max / cut_idem / relationPaths_of_allRel — a filter path is cut at its first non-relation field, idempotently, and all-relation paths are unchanged",
        axis: Lean,
        anchor: "crates/engine/query/src/planner/joins/relation_paths.rs Planner::relation_filter_paths, Planner::relation_prefix",
        model_ref: "JsonSubPathCut.cut_eq_longest / longest_allRel / longest_max / cut_idem / cut_of_allRel / relationPaths_of_allRel",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/planner/joins/relation_paths_tests.rs json_sub_path_is_not_a_relation_path, relation_path_stops_at_json_field, json_sub_path_filter_through_relation_plans",
    },
    Property {
        family: "Filter relation paths and scalar leaves",
        name: "newValidate_false_iff / newRun_eq_mainRun — validation rejects exactly a sub-selection on a non-relation field, and the queries that run are main's",
        axis: Lean,
        anchor: "crates/engine/query/src/runner/plan_validation.rs validate_select; crates/engine/query/src/planner/joins/selection_join.rs Planner::apply_selection_relation_join",
        model_ref: "JsonSubPathCut.leafOk_false_iff / newValidate_false_iff / newValidate_of_main / newRun_eq_mainRun",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/planner/joins/relation_paths_tests.rs sub_selection_on_json_field_is_go_validation_error, json_sub_path_filter_plans_without_index",
    },
    Property {
        family: "Relation filter with no matching child",
        name: "empty_cache_keeps_nothing / shortcut_same_rows / orphans_need_the_walk — a primary relation-filter join whose child cache is empty returns the rows of the full parent walk without reading a parent, and an exhaustive join, which also returns orphans, must not take that short cut",
        axis: Lean,
        anchor: "crates/engine/query/src/plan/type_join/type_join_one.rs TypeJoinOne::no_child_can_pass, TypeJoinOne::init, TypeJoinOne::next",
        model_ref: "RelationFilterNoChild.empty_cache_keeps_nothing / shortcut_same_rows / orphans_need_the_walk",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/planner/joins/relation_filter_execution_tests.rs no_matching_child_reads_no_parent, no_matching_child_reads_no_parent_of_an_index_ordered_scan",
    },
    Property {
        family: "Joins on a parent read by key",
        name: "lookup_same_rows / lookup_keeps_only_passing / raw_attach_keeps_a_rejected_parent / lookup_many_same_rows / lookup_many_keeps_only_passing / pass_through_drops_the_children — a parent an index-driven join reads by key is joined by the plan that join wraps, a list relation with all its children, so the rows are those of the full walk and a parent whose other relation fails its filter is not returned",
        axis: Lean,
        anchor: "crates/engine/query/src/plan/type_join/type_join_one.rs TypeJoinOne::join_fetched, TypeJoinOne::child_of_fetched, TypeJoinOne::next_inverted_index, TypeJoinOne::next_ordered_primary_inner; crates/engine/query/src/plan/type_join/type_join_many/plan_node.rs TypeJoinMany::join_fetched; crates/engine/query/src/plan/type_join/type_join_many/fetched.rs TypeJoinMany::join_children_of_fetched",
        model_ref: "JoinFetchedParent.lookup_same_rows / lookup_keeps_only_passing / raw_attach_keeps_a_rejected_parent / lookup_many_same_rows / lookup_many_keeps_only_passing / pass_through_drops_the_children",
        tiers: &[Contract],
        bound_by: "tools/integration-test/tests/testo/query/one_to_many/with_relation_filter_indexed_fk.rs relation_filter_through_an_indexed_fk_keeps_a_nested_joined_relation, relation_filter_through_an_indexed_fk_keeps_a_second_relation_filter, relation_filter_through_an_indexed_fk_keeps_a_joined_list_relation, relation_filter_through_an_indexed_fk_keeps_a_list_relation_filter",
    },
    Property {
        family: "Commits depth traversal",
        name: "minus_one_is_unlimited / other_negative_refused / nonnegative_is_itself / minus_one_clears_cap / walk_eq_fresh / walk_mem / walk_nodup / walk_unlimited / go_scan_merged / push_time_differs — `_commits(depth: -1)` reads as u64::MAX and so walks with no limit, every other negative depth is refused, and a non-negative one is itself; the stack walk below a cid, deciding each commit at pop time against the most depth it was walked with, lists the last-head-first path walk at first occurrence, each commit once, the same set as the old forward walk, and for an unlimited walk in Go's DAG-scan order, which marking at push time breaks, and keeps a commit only a shorter path reaches, which a plain seen set loses; a commit walked with at least UNLIMITED_DEPTH (2^62) left is never walked again, which -1 clears, so an unlimited walk loads each block once; every block is assumed to load",
        axis: Lean,
        anchor: "crates/database/db/src/read/commits/mod.rs CommitsFetcher::fetch_commit_by_cid, CommitsFetcher::traverse_depth, CommitsFetcher::push_heads; crates/engine/query/src/query_parse/parser.rs parse_field_to_select",
        model_ref: "CommitsDepthCast.minus_one_is_unlimited / other_negative_refused / nonnegative_is_itself / minus_one_clears_cap / run_proc / walk_eq_fresh / walk_mem / walk_nodup / walk_unlimited / go_scan_merged / walk_merged / go_scan_diamond / push_time_differs / walk_short_long / seen_only_loses_a_commit / ladder_loads_once / no_guard_loads_again",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/read/commits_depth_walk.rs commits_depth_walk_lists_each_commit_of_a_diamond_chain_once_last_head_first, commits_depth_walk_keeps_rows_a_shorter_path_reaches, commits_depth_walk_lists_a_long_history_at_unlimited_depth, commits_depth_walk_reads_each_block_once_at_unlimited_depth; crates/engine/query/tests/query_parse_tests.rs test_parse_commits_depth_minus_one_is_unlimited, test_parse_commits_depth_other_negative_returns_error",
    },
    Property {
        family: "PQ distance four sums",
        name: "four_eq_sum — four running sums over a code's lookups add up to the one running sum, in exact arithmetic",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/quantize/pq.rs ProductQuantizer::distance",
        model_ref: "PqFourSums.four_eq_sum / dropped_tail_loses_lookups",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/quantize_pq.rs the_lookup_table_agrees_with_the_reconstruction",
    },
    Property {
        family: "Query fragment nesting bound",
        name: "reach_le / walk_none_iff — the fragment walk never calls itself deeper than max_fragment_nesting, and refuses exactly the queries that nest deeper",
        axis: Lean,
        anchor: "crates/engine/query/src/query_parse/limits.rs FragmentPath::check_nesting; crates/engine/query/src/subscription.rs collect_root_fields",
        model_ref: "FragmentNesting.reach_le / walk_none_iff / off_by_one_accepts_too_deep",
        tiers: &[Contract],
        bound_by: "crates/engine/query/tests/fragment_stack.rs a_long_fragment_chain_is_refused_on_a_small_stack, a_chain_at_the_default_limit_still_parses",
    },
    Property {
        family: "Index-maintenance consistency",
        name: "onDocumentUpdate_correct / _no_stale / _none_missing",
        axis: Lean,
        anchor: "crates/database/db (index) index maintenance",
        model_ref: "IndexMaintenance (lake build)",
        tiers: &[Behavioral, Contract],
        bound_by: "proofs/tests/behavioral/index.rs index_no_stale_no_missing",
    },
    Property {
        family: "Secondary index equal-key order",
        name: "IndexEqualKeyOrder.exactMatch_ascending / exactMatch_creation_order / later_create_larger / docIdOrder_can_reverse_creation / inScan_scan_order_without_repeats / resolveUpTo_same_page: equal index keys come back in this node's short-ID order, which follows when each document was first stored on this node (created, or first received from a peer) rather than when its source created it, as Go returns them, paged without a reorder; an InScan lists each document once, at its first encounter; resolving only the page gives the page full resolution gives",
        axis: Lean,
        anchor: "crates/database/db/src/read/index_order.rs apply_exact_match_page, extend_unique, resolve_bound, resolve_in_order; crates/database/db/src/read/doc.rs get_by_index_scan; crates/database/db/src/read/lensed/fetcher/scan.rs get_by_index_scan_inner; crates/database/db/src/read/lensed/autocommit/scan.rs get_by_index_scan_inner; crates/database/db/src/docid/map.rs DocShortIdAllocator::next",
        model_ref: "IndexEqualKeyOrder (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/read/index_composite_in_order.rs partial_in_over_a_composite_index_keeps_secondary_field_order; tools/integration-test/tests/testo/encryption/peer_sec_index.rs doc_encryption_peer_if_encrypted_doc_has_indexed_field_should_index_after_decryption",
    },
    Property {
        // `index::index_reconciles_lww_merge_after_restart` drives a restart
        // partition where two replicas write different values for the same indexed
        // LWW field. Once the DAG converges, indexed filters must expose exactly
        // the winning value and no stale seed/loser keys.
        family: "Index-maintenance consistency",
        name: "INV_IndexMatchesWinner — indexed filters equal the converged CRDT value",
        axis: Tla,
        anchor: "crates/database/db (merge) LWW materialization; crates/database/db (index) on_document_update",
        model_ref: "MC_IndexReconciliation_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/index.rs index_reconciles_lww_merge_after_restart",
    },
    Property {
        // A text, JSON or custom field's merge writes its merged value into
        // the document before the composite updates the index, so the
        // update starts from a read taken before the field merges.
        family: "Index-maintenance consistency",
        name: "INV_IndexMatchesValue / INV_NoLostEdit — after a merge the index holds exactly the merged value, and every edit counts",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/composite_fields.rs process_linked_field_blocks; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document; crates/database/db/src/merge/merge_handler/block_reads.rs BlockReads::wrote_field; crates/database/db/src/merge/merge_handler/parked.rs replay_in_txn",
        model_ref: "MC_MergedFieldIndex_{Green,Green_Clear,Red_ReadAfterPart,Red_StaleCache}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Increments of an indexed counter append the document to a log
        // instead of moving the entry; reads check owed documents live and a
        // drain moves the entries later.
        family: "Index-maintenance consistency",
        name: "INV_ReadExact — a read through a deferred index finds exactly the matching documents",
        axis: Tla,
        anchor: "crates/database/db/src/index/deferred",
        model_ref: "MC_DeferredIndex_{Green,Green_Deep,Green_Drains,Red_NoOverlay,Red_NoOwnAppends,Red_UnvalidatedRecord,Red_TruncateHead,Red_ValidatedTxRecord}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A reindex clears the entries but keeps the records while drains run,
        // so the walk removes what the record names before it writes, and its
        // put of the record conflicts with a drain that moved it elsewhere.
        family: "Index-maintenance consistency",
        name: "INV_NoOrphanEntry — a reindex walk racing drains leaves no entry that no record names",
        axis: Tla,
        anchor: "crates/database/db/src/index/manager/deferred.rs IndexManager::replace; crates/database/db/src/index/manager/rebuild.rs IndexManager::reindex_document",
        model_ref: "MC_DeferredIndex_{Green_Reindex,Red_WalkOverwrite,Red_WalkBlindRecord}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A dropped index's log is truncated in pages from the first entry
        // left, so no transaction grows with the index's write history.
        family: "Index-maintenance consistency",
        name: "INV_CleanupBounded — no drop page truncates more than a page of log positions",
        axis: Tla,
        anchor: "crates/database/db/src/index/deferred/cleanup.rs remove_page; crates/database/db/src/index/drop.rs DropPage::run",
        model_ref: "MC_DeferredIndex_{Green_Drop,Red_DropWhole,Red_DropFromOne}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A document whose migration fails is appended to the log again in
        // the drain's own transaction, so the rest of the plan still drains.
        family: "Index-maintenance consistency",
        name: "PROP_HealthyDrains — a document that cannot be migrated never holds the others in a deferred index's log",
        axis: Tla,
        anchor: "crates/database/db/src/index/deferred/drain.rs apply",
        model_ref: "MC_DeferredIndex_{Green_Poison,Green_PoisonFixed,Red_PoisonAborts,Red_PoisonDropped,Red_FullPlanStuck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A plan's cap doubles while plans come back full, so a backlog drains
        // in fewer commits, and stops at the largest cap.
        family: "Index-maintenance consistency",
        name: "INV_PlanBounded — no drain plan takes more log entries than the largest cap",
        axis: Tla,
        anchor: "crates/database/db/src/index/deferred/drain.rs PlanSize::after; crates/database/db/src/index/deferred/task.rs drain_turn",
        model_ref: "MC_DeferredIndex_{Green,Green_Deep,Green_Poison,Red_UncappedPlan}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/index/deferred/plan_size.rs",
    },
    Property {
        // Whether an index is deferred follows the types a start registers,
        // so the check at open rebuilds one whose stored state is the other way.
        family: "Index-maintenance consistency",
        name: "INV_OpenMatchesMode — an open node's index holds exactly what the way it is kept accounts for",
        axis: Tla,
        anchor: "crates/database/db/src/definition/migration/format.rs DB::migrate_index_format; crates/database/db/src/database/mod.rs DB::finish_open; crates/database/db/src/index/deferred/mode.rs kept_in_other_mode; crates/database/db/src/definition/migration/reindex/page.rs clear; crates/database/db/src/definition/migration/reindex/progress.rs read",
        model_ref: "MC_DeferredIndexMode_{Green,Red_NoCheck,Red_OneWay,Red_KeepRecords,Red_NoProgressReset,Red_BackfillFirst}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/index/merged_crdt.rs",
    },
    Property {
        // Every write that owes a deferred index asks for its drain; the
        // drain task keeps a failed index for the sweep, and the sweep runs
        // on a fixed deadline that requests cannot push back.
        family: "Index-maintenance consistency",
        name: "INV_OwedKept — work a deferred index owes is always asked for or being drained",
        axis: Tla,
        anchor: "crates/database/db/src/index/deferred/task.rs",
        model_ref: "MC_DeferredDrainTask_{Green,Red_StopAtFailure,Red_DropFailed,Red_NoRequeue,Red_SilentPurge,Red_RearmSweep,Red_NoSweep,Red_RequestFirst,Red_SharedNote}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/index/deferred_task.rs",
    },
    Property {
        // A batch asks for each deferred index's drain once per attempt, not
        // once per document it owes.
        family: "Index-maintenance consistency",
        name: "INV_OneAsk — an attempt registers one drain callback per deferred index it owes",
        axis: Tla,
        anchor: "crates/database/db/src/index/manager/deferred.rs IndexManager::owe",
        model_ref: "MC_DeferredDrainTask_{Green,Red_AskPerDoc}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/index/deferred/requests.rs",
    },
    Property {
        // The drain finds a document's old entries only through its position
        // record, so the record must read back as the keys it names.
        family: "Index-maintenance consistency",
        name: "decode_encode — a position record reads back as exactly its entry keys",
        axis: Lean,
        anchor: "crates/database/db/src/index/deferred/record.rs encode_record, decode_record",
        model_ref: "PositionRecord (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A node's shutdown aborts every task it holds and waits for each to
        // let go of storage before the pool stops and regolith closes; the
        // drain gives up at its deadline; a refused telemetry flush moves to
        // a thread of its own instead of the caller. Requests and in-process
        // writes stop and drain before P2P, so every admitted write registers
        // its head marker; the transactions callers left open end before
        // regolith closes.
        family: "Node shutdown order",
        name: "INV_NoStorageAfterClose / INV_NothingDetached / INV_CallerNeverJoinsExporter / INV_AdmittedWritesMarked / INV_CloseWaitsForNoTransaction / LIVE_ShutdownTerminates: no task reaches storage after regolith closes, none outlives shutdown, every admitted write is marked, the close waits for no transaction left open, shutdown returns",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::shutdown, Node::as_request; crates/entry-points/defra-node/src/ops/request.rs Node::txn_begin, Node::txn_commit; crates/entry-points/defra-node/src/network.rs NodeEdge::drain_requests; crates/surfaces/http/src/server.rs Server::router; crates/entry-points/defra-node/src/node_acp.rs gate_external_acp; crates/entry-points/defra-node/src/managed_p2p.rs ShutdownHandle::stop, stop_services; crates/foundation/edge/src/task.rs Service::stopped; crates/foundation/edge/src/handle.rs Scope::drain; crates/building-blocks/net/src/server.rs serve, until_closing",
        model_ref: "MC_NodeShutdown_{Green,Red_RedriveDetached,Red_ServicesNotAwaited,Red_NoDrainDeadline,Red_FlushOnCaller,Red_RequestsDuringP2P,Red_CommitNotAdmitted,Red_OpenTransaction}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A node dropped without shutdown aborts every task its shutdown set
        // holds, the transaction sweep among them, without waiting for any,
        // so its pool's threads and its store are released.
        family: "Node shutdown order",
        name: "LIVE_DroppedNodeReleasesPool / INV_DropNeverWaits / INV_ShutdownStopsEveryTask: a dropped node releases its pool and store, its drop waits for no task, a shutdown stops every task",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/assemble.rs assemble; crates/entry-points/defra-node/src/background_tasks.rs BackgroundTasks; crates/database/db/src/txn/registry/cleanup.rs start_stale_transaction_cleanup; crates/foundation/workers/src/pool/background.rs Background",
        model_ref: "MC_NodeDrop_{Green,Red_SweepOutsideSet,Red_NoAbortOnDrop,Red_DropAwaits}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/node_drop.rs a_dropped_http_node_releases_its_threads_and_descriptors",
    },
    Property {
        // A node that runs its own Vera node stops it after the request
        // drain, waits for it up to a deadline and closes its transports:
        // a remembered decision outlives it by no more than the freshness of
        // the state it was read at, and once it has ended its
        // directory is free for the next node. A Vera node that hangs past
        // the deadline keeps its directory.
        family: "Node shutdown order",
        name: "INV_NoDecisionPastFreshness / INV_AdmittedRequestsDecided / INV_StoppedNodeFreesDirectory / LIVE_ShutdownReturns / LIVE_NextStartReturns: once the embedded Vera node is gone a remembered access decision is repeated no longer than its state stays fresh and no new one is given, shutdown stops it under no admitted request but one the request drain gave up on, a node that shut down holds the directory of an ended Vera node no longer, shutdown returns, the next start over the directory starts or fails at its deadline",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::shutdown; crates/entry-points/defra-node/src/embedded_vera.rs start; crates/entry-points/cli/src/commands/start/run.rs Node::run; crates/entry-points/cli/src/commands/start/server_acp_embedded.rs setup; crates/engine/vera/src/vera_rs/provider.rs decision_state; crates/engine/vera-embedded/src/host.rs HostedNode::start, HostedNode::stop; crates/engine/vera-embedded/src/link.rs Link; crates/engine/vera-embedded/src/call.rs call",
        model_ref: "MC_EmbeddedVeraStop_{Green,Red_StaleDecision,Red_StopBeforeRequests,Red_NoStopDeadline,Red_LinkStaysOpen,Red_NoStartDeadline}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera-embedded/tests/hosted_node.rs once_the_node_stops_the_provider_verifies_no_new_decision; crates/engine/vera-embedded/tests/stopped_node_decisions.rs a_remembered_decision_is_not_given_past_the_freshness_limit",
    },
    Property {
        // A signed query or transformed route keeps a request-scope permit
        // while its started worker writes, so P2P cannot stop first.
        family: "Node shutdown order",
        name: "INV_CommittedMarked: a signed query or transformed route commits with its durable head marker",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/surfaces/http/src/server.rs Server::router; crates/engine/query/src/pooled/mod.rs PooledExecutor::execute_prepared; crates/foundation/workers/src/pool/detached.rs run_on; crates/entry-points/defra-node/src/network.rs NodeEdge::drain_requests",
        model_ref: "MC_SignedRequestDrain_{Green,Red_TaskDeadline,Red_Transform}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/signed_query/tests.rs a_signed_query_remains_admitted_past_the_edge_deadline; crates/entry-points/defra-node/tests/unit/verified_create.rs signed_writes_are_refused_once_requests_have_drained; crates/surfaces/http/tests/request_admission.rs a_route_added_by_router_transform_stays_admitted_until_it_ends",
    },
    Property {
        // A transaction guard follows its admitted commit task rather than a
        // caller that can stop waiting while the task still runs.
        family: "Node shutdown order",
        name: "INV_CommitNotAbandoned: cancelling the waiter does not abandon an edge-admitted commit in flight",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/transaction.rs NodeTransaction::commit; crates/engine/query/src/txn/guard.rs TransactionGuard::drop; crates/database/db/src/txn/owner/table.rs Table::abandon",
        model_ref: "MC_GuardedCommit_{Green,Red_CallerOwns}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/transaction/cancellation_tests.rs cancelling_a_commit_waiter_does_not_abandon_the_running_commit",
    },
    Property {
        // Request admission follows the worker; transaction ownership follows
        // its commit task when the caller gives up waiting.
        family: "Node shutdown order",
        name: "start_keeps_permit / finish_keeps_permit / dropped_waiter_keeps_permit / expired_task_loses_permit / task_guard_survives_cancellation / caller_guard_is_abandoned / begin_open_has_owner / abandoned_begin_is_closed",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/entry-points/defra-node/src/transaction.rs NodeTransaction::commit; crates/foundation/workers/src/pool/detached.rs run_on",
        model_ref: "RequestDrain (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/signed_query/tests.rs a_signed_query_remains_admitted_past_the_edge_deadline; crates/surfaces/http/tests/request_admission.rs a_route_added_by_router_transform_stays_admitted_until_it_ends; crates/entry-points/defra-node/src/transaction/cancellation_tests.rs cancelling_a_commit_waiter_does_not_abandon_the_running_commit, cancelling_a_begin_waiter_abandons_the_unreturned_transaction",
    },
    Property {
        // The result of a detached begin abandons its transaction if its
        // caller leaves before receiving the handle.
        family: "Node shutdown order",
        name: "INV_OpenHasOwner: an edge-admitted begin cannot leave an unowned transaction open",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::begin_transaction; crates/entry-points/defra-node/src/ops/request.rs Node::txn_begin; crates/engine/query/src/txn/guard.rs TransactionGuard::drop",
        model_ref: "MC_DetachedBegin_{Green,Red_NoResultGuard}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/transaction/cancellation_tests.rs cancelling_a_begin_waiter_abandons_the_unreturned_transaction",
    },
    Property {
        // A network task's completion says how the work ended: its own
        // handle's abort is Cancelled, the runtime's stop is Closed, a value
        // is a value. The handle records its abort before it cancels, and the
        // runtime marks itself stopped before it ends its tasks.
        family: "Edge completion",
        name: "INV_AbortSaysCancelled / INV_StopSaysClosed / INV_DoneSaysValue: a completion its handle aborted says Cancelled, one the runtime's stop cut off says Closed, one that finished says its value",
        axis: Tla,
        anchor: "crates/foundation/edge/src/task.rs Task::poll, Task::abort; crates/foundation/edge/src/platform/native.rs the shutdown; crates/foundation/edge/src/shared.rs Shared::stop",
        model_ref: "MC_EdgeCompletion_{Green,Red_NoMap,Red_CancelBeforeFlag,Red_CancelBeforeStopMark}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/foundation/edge/src/tests/task_set.rs aborting_a_set_stops_its_tasks_and_hands_each_back_cancelled; shutdown.rs shutdown_stops_accepting_drains_then_stops_the_rest; spawn.rs a_spawned_task_hands_back_its_value",
    },
    Property {
        // Work that continues a request already admitted (an outbound
        // connection's driver) is admitted after the gate closes while
        // admitted work is in flight; the drain seals before its last look, so
        // it never says drained while a continuation still runs.
        family: "Edge gate continuation",
        name: "INV_NothingAfterDrain / INV_InFlightContinues / INV_NoContinuationWhenIdle: nothing holds a permit once drained, a request in flight gets its continuation, and none starts from an idle closed gate",
        axis: Tla,
        anchor: "crates/foundation/edge/src/gate.rs Gate::admit, Gate::admit_continuation, Gate::drained; crates/foundation/edge/src/handle.rs Edge::spawn_continuation, drain_gate; crates/building-blocks/net/src/rt.rs ClientExecutor::execute",
        model_ref: "MC_EdgeGateContinuation_{Green,Red_PlainSpawn,Red_NoSeal,Red_NoCas}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/foundation/edge/tests/continuation.rs a_continuation_is_admitted_after_close_while_admitted_work_runs, a_continuation_is_refused_after_close_when_nothing_is_in_flight, the_drain_waits_for_a_running_continuation, a_continuation_is_refused_after_the_drain_completed_even_with_plain_spawns_racing; crates/building-blocks/net/tests/server.rs a_closing_client_finishes_the_request_in_flight_and_frees_its_idle_connections, a_request_in_flight_at_close_opens_a_fresh_connection_and_finishes",
    },
    Property {
        // An iroh dial answers from its connection's outcome: a refusal that
        // already closed the connection is answered Unauthorized, and
        // connect_peer ends with the dial's answer instead of polling the
        // connected set until a timeout.
        family: "Iroh dial outcome",
        name: "INV_RefusalKnownIsSaid / LIVE_DialAnswered: a dial never answers ok on a connection the peer's refusal already closed, and every dial is answered",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_commands.rs handle_dial; crates/surfaces/p2p-adapter/src/iroh.rs IrohP2PAdapter::connect_peer; crates/database/p2p/src/iroh/transport.rs poll_until_connected",
        model_ref: "MC_IrohDialOutcome_{Green,Green_Admitted,Red_Poll,Red_NoCheck}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/iroh/allowlist_tests.rs closed_by_refusal_reads_the_remote_close_code, poll_until_connected_wakes_on_the_connection_and_times_out_without_one",
    },
    Property {
        // A server's connections are tasks the drain waits for: a request in
        // flight when shutdown begins finishes and is answered, a connection
        // looks at closing only between requests, and the drain's deadline
        // stops what is left. A subscription stream, which has no end of its
        // own, ends with its complete event once closing began, so the
        // deadline stops no stream; it may still stop a slow or stuck
        // ordinary request.
        family: "Server drain",
        name: "INV_NoDropBeforeDeadline / INV_NoNewWorkWhenClosing / INV_DeadlineStopsNoStream / LIVE_ServerReturns: a request in flight is only stopped at the drain's deadline, no request starts once closing began, the deadline stops no subscription stream, the server returns",
        axis: Tla,
        anchor: "crates/building-blocks/net/src/server.rs serve, until_closing; crates/building-blocks/net/src/rt.rs Executor::execute; crates/database/pg-compat/src/lib.rs PgServer::run; crates/database/pg-compat/src/wire/process.rs serve; crates/foundation/edge/src/handle.rs Scope::drain; crates/surfaces/http/src/handlers/closing.rs next_unless_closing; crates/surfaces/http/src/handlers/graphql/query.rs graphql_sse; crates/surfaces/http/src/handlers/events.rs events_sse",
        model_ref: "MC_ServerDrain_{Green,Red_Service,Red_CheckInside,Red_NoCheck,Red_NoDeadline,Red_StreamIgnoresClosing}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/building-blocks/net/tests/server.rs a_closing_server_finishes_the_requests_in_flight_then_returns; crates/database/pg-compat/tests/wire.rs a_query_in_flight_when_the_edge_starts_closing_completes_and_the_connection_closes, an_idle_connection_closes_at_once_when_the_edge_starts_closing; crates/surfaces/http/tests/sse_ends_when_closing.rs a_graphql_subscription_ends_when_its_edge_closes, the_event_stream_ends_when_its_edge_closes; crates/entry-points/defra-node/tests/subscription_shutdown.rs shutdown_ends_an_open_graphql_subscription_without_waiting_out_the_drain",
    },
    Property {
        // An update on a document's topic is applied only while the node
        // follows the document or its collection: an unsubscribe clears the
        // set the gossip handler reads before it returns, so updates queued or
        // sent after it are dropped.
        family: "Document topic ingress",
        name: "INV_NoApplyAfterUnsubscribe / INV_FollowedApplies: no update is applied after the document's unsubscribe returned unless its collection is followed, and none taken while followed is dropped",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/event_handler/gossip.rs handle_gossip_message; crates/database/p2p/src/sync/coordinator/access.rs is_followed_document; crates/surfaces/p2p-adapter/src/libp2p.rs P2PAdapter::add_documents, remove_documents; crates/surfaces/p2p-adapter/src/iroh.rs IrohP2PAdapter::add_documents, remove_documents; crates/entry-points/defra-node/src/libp2p_recovery.rs restore_libp2p_documents; crates/surfaces/p2p-adapter/src/iroh_restore.rs restore_iroh_p2p_state",
        model_ref: "MC_DocTopicIngress_{Green,Red_NoCheck,Red_OwnSet,Red_NoCollection}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/access_tests.rs gossip_document_topic_dropped_after_document_unfollowed; crates/surfaces/p2p-adapter/tests/libp2p/followed_documents.rs remove_documents_clears_the_shared_followed_set",
    },
    Property {
        // A node checks its config before it opens anything, hands
        // its retry ladder to every retry site of either transport, stops a
        // host whose listen failed, enables NAC owned by the node identity,
        // and dials bootstrap peers only once NAC is in place, logging a peer
        // that does not answer.
        family: "Node startup",
        name: "INV_FailedHoldsNothing / INV_OneSchedule / INV_NacAsAsked / INV_NoPeerBeforeNac / INV_BootstrapNonFatal / LIVE_StartupEnds: a failed start holds nothing, a started node is configured as asked",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/assemble.rs assemble; crates/entry-points/defra-node/src/libp2p_runtime.rs setup_libp2p, assemble_libp2p; crates/entry-points/defra-node/src/p2p_runtime.rs setup_iroh; crates/entry-points/defra-node/src/node_acp.rs create_nac_manager",
        model_ref: "MC_NodeStartup_{Green,Red_IrohDefaultLadder,Red_ListenLeak,Red_LateNacCheck,Red_NacUnowned,Red_DialBeforeNac,Red_BootstrapFatal}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Node startup",
        name: "new_keeps / new_valid / new_accepts_valid / cap_mem / cap_positive / cap_follows_ladder / cap_repeats_last / default_accepted",
        axis: Lean,
        anchor: "crates/engine/storage/src/stores/retry_info.rs RetrySchedule::new, RetrySchedule::cap, RETRY_INTERVALS_SECS",
        model_ref: "RetrySchedule (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Inbound stream admission",
        name: "INV_ResidentWithinBudget / INV_ShareBound / LIVE_HealthyHandled / LIVE_WaitEnds",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_streams.rs admit_stream; crates/database/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, InboundBudget::peer_share, InboundDeadlines::body; crates/database/p2p/src/sync/dispatch_queue.rs dispatch_queue; crates/database/p2p/src/sync/event_dispatcher.rs run_event_dispatcher",
        model_ref: "MC_InboundAdmission_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A request shed while every rejection worker is busy is dropped. Its
        // stream closes empty, so the requester sees Refused (CAR) or
        // Transport (PushLog) instead of Busy, and spends free refusals, stall
        // budget and an attempt. Retries stay bounded by DagFetchBudget and the
        // per-root clock.
        family: "Inbound stream admission",
        name: "A request shed past the rejection workers is answered by a closed stream, not Busy",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/event_dispatcher.rs run_event_dispatcher, MAX_ACTIVE_REJECTIONS; crates/database/p2p/src/iroh/endpoint_rpc.rs CarFetchOutcome",
        model_ref: "MC_InboundAdmission_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Inbound stream admission",
        name: "INV_OnlyRegisteredServed / INV_ShareBound / INV_RoomIsTheRequest / INV_RevokedIsCut / INV_ShutdownLeavesNone / LIVE_HandlerEnds",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_streams.rs admit_stream, handle_connection_streams, serve_protocol_stream; crates/database/p2p/src/iroh/stream_protocols.rs StreamProtocols::get, ProtocolRequest::release, StreamHandler::handle; crates/database/p2p/src/iroh/endpoint_commands.rs handle_deny_peer; crates/database/p2p/src/iroh/task_registry.rs TaskRegistry::close",
        model_ref: "MC_EmbedderStream_{Green,Red_AnyTag,Red_UntilReturn,Red_NoDeadline,Red_BarOnly,Red_NoAbort}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Inbound stream admission",
        name: "register_all_valid / register_serves_each_as_given / reserved_tags_find_nothing / unregistered_tags_find_nothing / a_repeated_tag_is_refused",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/stream_protocols.rs StreamProtocol::validate, StreamProtocols::new, StreamProtocols::get",
        model_ref: "StreamProtocols (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Inbound stream admission",
        name: "acquire_conserves / release_all_restores / admit_failure_keeps_nothing",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, InboundLease",
        model_ref: "ByteBudget (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer fetch stats",
        name: "roll_ratio_stays_a_ratio / rolling_over_samples / bump_never_overflows",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/fetch_stats.rs PeerCounters::record, roll",
        model_ref: "FetchStats (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "DAG fetch provider ranking",
        name: "INV_WaitBounded — one attempt waits at most a rotation of stalls plus its free refusals, however many batches",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_fetcher.rs fetch_dag_attempt, poll_fetch_blocks_rotating, window_outcome; crates/database/p2p/src/sync/coordinator/dag_retry.rs FAST_REFUSAL, MAX_FREE_REFUSALS",
        model_ref: "MC_DagFetchBudget_{Green,Red_SlowIsRefusal,Red_Uncapped,Red_NoCheck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "DAG fetch provider ranking",
        name: "rank_perm / rank_keeps_healthy_source_first / rank_all_healthy / next_delay_in_range / stale_starts_over / forget_keeps_fresh / forget_drops_stale",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/provider_health.rs ProviderHealth::rank, record_fast_failure, forget_stale, Cooldown::is_stale",
        model_ref: "ProviderHealth (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "DAG fetch arbitration (EWMA)",
        name: "ArbitrationConstants: VALUE_BITS, TIME_BITS, WEIGHT_ONE, the half-lives, shifts, tick, TIE_BUCKET, BACKOFF_FLOOR, units, floor and WEIGHT_TABLE, with table_is_floor / table_decreasing / table_wraps / packed_word_fits / tie_bucket_le_backoff_floor / useful_floor_is_one_64th / occupancy_half_life_ticks checked on exactly these values",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/arbitration/mod.rs VALUE_BITS, TIME_BITS, WEIGHT_ONE, WEIGHT_TABLE, PROVIDER_HALF_LIFE; crates/database/p2p/src/sync/arbitration/ewma.rs weight_q32; proofs/tests/lean_conformance.rs lean_arbitration_constants_match_rust",
        model_ref: "Arbitration.Constants (lake build); Conformance.lean ArbitrationConstants",
        tiers: &[Contract],
        bound_by: "proofs/tests/lean_conformance.rs lean_arbitration_constants_match_rust",
    },
    Property {
        family: "DAG fetch arbitration (EWMA)",
        name: "roll_le_bound / troll_le_bound / record_first / troll_full / iter_monotone_up / iter_stays_below / decay_geometric / troll_decay_up / troll_decay_down / dither_mean / weight_le_one / weight_zero_gap / weight_first_ms / expected_useful_le / bucket_order_any_score / inversion_within_bucket / rearmed_root_goes_behind",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/arbitration/ewma.rs roll, troll, record_word, PackedEwma; root.rs RootEstimates::observe; occupancy.rs advance; provider.rs ProviderEstimates, expected_useful; due_order.rs tie_break_order; tests.rs; tests/unit/estimator_trace_replay.rs; tests/unit/estimators_observe_only.rs",
        model_ref: "Arbitration (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "INV_RevokedHasNoGossip / INV_PathOnLiveConnection / INV_GossipNeverClosesShared / LIVE_HealthyReached / LIVE_AskAnswered",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/gossip/driver.rs Driver::send, Driver::connected, Driver::forget; crates/database/p2p/src/iroh/gossip/mod.rs GossipHandle::connected; crates/database/p2p/src/iroh/endpoint_rpc.rs connect_with_cache; crates/database/p2p/src/iroh/endpoint_commands.rs handle_deny_peer",
        model_ref: "MC_GossipOwnership_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "LIVE_RequestAnswered",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/gossip/inbox.rs Inbox::push, Inbox::offer, Inbox::next; crates/database/p2p/src/iroh/gossip/driver.rs read_stream, Driver::emit; crates/database/p2p/src/iroh/records/task.rs RecordsHandle::push_change, RecordsHandle::push, RecordsHandle::next",
        model_ref: "MC_GossipInbox_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "decode_encode / decode_all_encode_all / decode_rejects_oversized / read_tag_write_tag",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/gossip/frame.rs write_gossip_frame, read_gossip_frame; crates/database/p2p/src/iroh/protocols.rs write_stream_tag, read_stream_tag",
        model_ref: "GossipFrame (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "broadcast_fits_frame / emitted_fits_frame / max_broadcast_fits / over_max_broadcast_dropped / sealed_raw_fits",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/gossip/frame.rs MAX_MESSAGE_SIZE, TOPIC_ID_LEN, FRAME_LIMIT, BROADCAST_OVERHEAD, MAX_BROADCAST, max_broadcast; crates/database/p2p/src/iroh/gossip/driver.rs config; crates/database/p2p/src/iroh/gossip/raw_origin.rs HEADER_LEN, MAX_RAW_PAYLOAD; crates/database/p2p/src/iroh/transport.rs publish_raw",
        model_ref: "GossipFrame.Budget (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/iroh/direct_push.rs a_raw_publish_too_large_for_gossip_is_refused",
    },
    Property {
        family: "Gossip connection ownership",
        name: "INV_NoSilentLoss / INV_BroadcastFits",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/gossip/mod.rs GossipHandle::publish; crates/database/p2p/src/iroh/gossip/driver.rs write_stream, write_queue; crates/database/p2p/src/iroh/endpoint_commands.rs handle_command; crates/database/p2p/src/iroh/direct_push.rs peers, push, push_one, FANOUT",
        model_ref: "MC_GossipDelivery_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh service records",
        name: "INV_SeqNeverGoesBack / INV_NoExpiredChosen / LIVE_NewestReached",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/records/book.rs Book::offer, Book::live; crates/database/p2p/src/iroh/records/refresh.rs Refresh::check; crates/database/p2p/src/iroh/records/task.rs Records::renew, Records::on_input, Records::rotate",
        model_ref: "MC_ServiceRecords_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A node publishes a service record only after its embedder sets a
        // ServiceProfile, and only the CLI does. For NodeBuilder and FFI
        // nodes LIVE_NewestReached is vacuous.
        family: "Iroh service records",
        name: "LIVE_NewestReached and LIVE_Found assume the embedder sets a ServiceProfile",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/records/task.rs Records::renew; crates/entry-points/cli/src/commands/start/server_p2p/iroh.rs",
        model_ref: "MC_ServiceRecords_Green.cfg / MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Depth-cut DAG fetch",
        name: "INV_HistoryNotResent / INV_RoundsLogarithmic / LIVE_Completes",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/car.rs collect_dag_blocks_within; crates/database/p2p/src/sync/coordinator/closure_depth.rs ClosureDepth::widen; crates/database/p2p/src/sync/coordinator/dag_fetcher.rs fetch_dag_attempt",
        model_ref: "MC_DepthCutFetch_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Depth-cut DAG fetch",
        name: "within_iff_level / deep_block_not_served / within_le / cut_le_cap / cut_ge_pow / cut_reaches",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/car.rs collect_dag_blocks_within; crates/database/p2p/src/sync/coordinator/closure_depth.rs ClosureDepth::initial, ClosureDepth::widen",
        model_ref: "CarClosureDepth (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // libp2p CAR bodies have one fixed total read deadline. A response
        // slower than CAR_MAX_BYTES over that deadline fails, and the fetch
        // falls back to Bitswap.
        family: "Depth-cut DAG fetch",
        name: "LIVE_Completes on libp2p assumes a CAR body arrives within the stream read deadline",
        axis: Tla,
        anchor: "crates/database/p2p/src/two_stream/handler/car.rs read_stream; crates/database/p2p/src/two_stream/runner.rs TwoStreamRunner::new",
        model_ref: "MC_DepthCutFetch_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Have-set DAG fetch",
        name: "INV_MergeComplete / INV_NoForged / INV_HeldNotResent / INV_RoundsBounded / INV_Converges",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/car/region.rs collect_missing_region, Walk::mark_held, Walk::mark_wanted, Walk::serve_attachments; crates/database/p2p/src/sync/coordinator/haves.rs haves_from_heads; crates/database/p2p/src/sync/coordinator/dag_fetcher.rs fetch_dag_attempt, root_haves; crates/database/p2p/src/sync/coordinator/event_handler/car.rs drop_held_blocks",
        model_ref: "MC_ExactDagFetch_{Green,Green_Liar,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Have-set DAG fetch",
        name: "INV_OneMarkWriter / INV_HeldNeverContends / INV_Lands",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/event_handler/car.rs drop_held_blocks; crates/database/p2p/src/sync/manager/process/mod.rs try_acquire_car_storage_owners; crates/engine/storage/src/stores/blockstore.rs BlockWritesAttempt",
        model_ref: "MC_HeldBlockIngest_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Have-set DAG fetch",
        name: "wanted_served / served_held_is_asked / missing_is_served / receiver_complete / reach_lowers_priority / first_is_root / decode_encode",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/car/region.rs collect_missing_region; crates/database/p2p/src/two_stream/handler/car.rs encode_car_request, decode_car_request",
        model_ref: "CarMissingRegion (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Clock and PushLog fetches single-flight per root in
        // PendingDagFetchTask, separately, and may overlap a reply fetch of the
        // same root. CAR ingest serializes the overlap (StorageContended
        // defers it); it costs bandwidth and a limiter permit, never a head.
        family: "Reply-driven DAG fetch",
        name: "INV_OneFetchPerRoot per claim key (root, replier's sorted explicit-replicator collection set), reply-driven fetches only / INV_NoReplyLost",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs DagFetchLimiter::claim_root, RootKey, RootClaim, PendingDagFetchTask; crates/database/p2p/src/sync/coordinator/event_handler/doc_sync.rs handle_doc_sync_reply; crates/database/p2p/src/sync/coordinator/event_handler/branchable_sync.rs handle_branchable_sync_reply; crates/database/p2p/src/sync/coordinator/event_handler/car.rs CarIngestDisposition::StorageContended; crates/database/p2p/src/sync/manager/process/mod.rs try_acquire_car_storage_owners",
        model_ref: "MC_RootSingleFlight_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Pull fetches are best-effort. A reply that cannot claim its root is
        // dropped, and the claimant tries only its replier and up to
        // MAX_PENDING_DAG_ALTERNATE_PROVIDERS connected alternates. Deferred,
        // Busy or no provider ends the pull with no re-drive: sync_documents
        // and BranchableSync callers own completion.
        family: "Reply-driven DAG fetch",
        name: "INV_NoReplyLost assumes the running fetch reaches a holder; a pull it fails is not re-driven",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::with_alternate_providers; crates/database/p2p/src/sync/pending_store/mod.rs MAX_PENDING_DAG_ALTERNATE_PROVIDERS; crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag",
        model_ref: "MC_RootSingleFlight_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh service records",
        name: "INV_AskOnlyWhenEmpty / LIVE_Found",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/records/task.rs Records::providers, Records::on_message",
        model_ref: "MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A lookup asks only when the book has no live record for the
        // collection key. Records name holders of a collection, not of a root,
        // so a non-holder's live record suppresses Who.
        family: "Iroh service records",
        name: "LIVE_Found assumes a live record's peer holds the root, not only the collection",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/records/task.rs Records::providers, Records::on_message",
        model_ref: "MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Dropping an index while its backfill runs: the drop's commit takes
        // the backfill record with it, a backfill failure is recorded only
        // while the index is defined, entries go in pages after the
        // definition commits, the drop retires the index's id so a new
        // index never shares its keys, and a restart finishes an unfinished
        // drop.
        family: "Index drop during backfill",
        name: "INV_NoRecordForGoneIndex / INV_NoOrphanEntries / INV_NewIndexIntact / LIVE_DropFinishes",
        axis: Tla,
        anchor: "crates/database/db/src/index/drop.rs DB::drop_index, DropIndexAttempt, DropPage, DB::remove_dropped_entries, DB::resume_index_drops; crates/database/db/src/index/manager/mod.rs index_id_floor; crates/database/db/src/index/backfill.rs BackfillPage::index, DB::backfill_index, FailBackfillAttempt, DB::resume_index_backfills",
        model_ref: "MC_IndexDropBackfill_{Green,Red_KeepRecord,Red_BlindFail,Red_SameTxn,Red_Forget,Red_ReuseId}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Closing the database while an index build waits at its build
        // gate: the wait also ends on the close, so the pool, which waits
        // for the build task, stops; the stopped build keeps its record in
        // progress, as after a crash; the next open returns without waiting
        // for it, and the node then resumes it as a task of its own, so the
        // build finishes once the gate opens.
        family: "Index build gate and close",
        name: "INV_BuiltOnlyWhenIndexed / LIVE_CloseFinishes / LIVE_BuildFinishes",
        axis: Tla,
        anchor: "crates/database/db/src/index/backfill.rs DB::backfill_batches, DB::backfill_index, DB::start_resumed_index_backfills, DB::resume_index_backfills; crates/database/db/src/database/mod.rs DB::close, DB::closing, DB::finish_open; crates/entry-points/defra-node/src/assemble.rs assemble; crates/database/db/src/index/build_gate.rs IndexBuildGate::enter",
        model_ref: "MC_IndexBuildGateClose_{Green,Red_NoRace,Red_FailOnClose,Red_Forget,Red_OpenAwaits}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Purging documents by DocID: values, index entries, the primary
        // key, heads and DocID aliases go in one transact attempt; outside a
        // branchable collection every block only the purged documents own
        // goes too, decided by a validated owner scan; in a branchable
        // collection the history stays and no owner edge names a purged
        // document; the count is the distinct stored documents, taken at
        // commit; a sync re-storing a walked block re-runs the attempt.
        family: "Purge by DocID",
        name: "INV_OthersIntact / INV_CollectionIntact / INV_Gone / INV_NoStaleOwner / INV_Pruned / INV_Count / PROP_Commits",
        axis: Tla,
        anchor: "crates/database/db/src/collection/ops/purge.rs DB::purge_by_doc_ids_as, DB::purge_documents; crates/database/db/src/txn/attempt.rs DB::transact_db; crates/database/db/src/collection/ops/truncate/filtered.rs DB::truncate_filtered_chunk; crates/database/db/src/txn/registry/purge.rs DbTransactionRegistry::purge_by_doc_ids_as; crates/database/db/src/block/cleanup.rs delete_owned_dag_for_owners, drop_dag_owners; crates/database/db/src/docid/map.rs block_has_owners",
        model_ref: "MC_Purge_{Green_Prune,Green_Branchable,Red_Blind,Red_PruneBranchable,Red_KeepEdges,Red_KeepIndex,Red_CountRequests,Red_CountAttempt,Red_Fail,Red_PlainScan}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The orphan block sweep: a fetched block's to-merge mark
        // holds its fetch time and a refetch leaves it alone; the sweep nominates
        // expired marks from a snapshot and decides each in a transaction
        // that reads the mark and the owners again, deleting an unowned
        // block, clearing the mark of an owned one, and never touching an
        // untimed mark; a merge clears and owns its signature block too.
        family: "Orphan block sweep",
        name: "INV_NoMergedLoss / INV_NoOwnedLoss / INV_MergedUnmarked / INV_SigOwned / INV_LegacyKept / PROP_Reclaims",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/blockstore.rs BlockWritesAttempt::run, marker_value, marker_time; crates/database/db/src/block/orphan_gc.rs DB::reclaim_orphan_blocks, DB::decide_orphans; crates/database/db/src/merge/merge_handler/doc_identity.rs record_block_ownership",
        model_ref: "MC_OrphanGc_{Green,Red_SnapshotRead,Red_NoOwnerCheck,Red_BlindRefresh,Red_ReclaimLegacy,Red_SigMarked,Red_SigUnowned}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The sweep does not read the pending-DAG store, so it reclaims the
        // fetched blocks of a durable obligation stalled past the TTL. The
        // model's weak fairness on Fetch assumes such a block can be fetched
        // again.
        family: "Orphan block sweep",
        name: "PROP_Reclaims assumes a reclaimed unmerged block can be fetched again",
        axis: Tla,
        anchor: "crates/database/db/src/block/orphan_gc.rs DB::decide_orphans, OrphanSweepConfig::TTL",
        model_ref: "MC_OrphanGc_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Orphan block sweep",
        name: "time_value / time_short / time_long / time_legacy",
        axis: Lean,
        anchor: "crates/engine/storage/src/stores/blockstore.rs marker_value, marker_time",
        model_ref: "OrphanMarker (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The replication filter (#2451): asked between the merge's phases,
        // so a refused document stores nothing; a refusal skips only its own
        // event, for good, and a later commit of a refused document is refused
        // too; a branchable collection is not filtered. A non-genesis commit
        // that finds no live heads is taken to follow a refusal: that is sound
        // only while a refusal is the only thing that leaves a document
        // headless, and a purge or a predecessor skipped non-terminally also do.
        family: "Replication filter",
        name: "INV_RefusedNothingStored / INV_KeptMerged / INV_MergedOnce / INV_BranchableUnfiltered",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/composite_fields.rs process_linked_field_blocks; crates/database/db/src/replication_filter.rs add_field_values; crates/database/db/src/merge/merge_handler/batch.rs try_batch_merge",
        model_ref: "MC_ReplicationFilter_{Green,Green_Branchable,Red_AfterPersist,Red_WholeBatch,Red_Retry,Red_FilterBranchable,Red_Alone}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Document write pipeline",
        name: "INV_NoStoreBeforeComposite / INV_NoMintInAttempt / INV_OneKeyPerField / INV_CommittedWhole",
        axis: Tla,
        anchor: "crates/database/db/src/write/create.rs embed, identity, seed_identity, memo_identity, prepare_keys, compute, compute_many, persist; crates/database/db/src/write/autocommit/create.rs create_many_with_resolved_inputs, create_impl, run_create_attempt; crates/database/db/src/write/autocommit/batch.rs BatchMutator::create_as (with a memo); crates/database/db/src/write/doc.rs DbDocMutator::create_as, next_create_identity; crates/database/db/src/write/update.rs write_local, write_blocks; crates/database/db/src/write/autocommit/update.rs run_update_attempt; crates/database/db/src/write/autocommit/batch.rs BatchMutator::update_as (with a memo); crates/database/db/src/write/doc.rs DbDocMutator::update_as; crates/database/db/src/block/builder/write.rs build_document_blocks, encrypt_with_new_key",
        model_ref: "MC_WritePipeline_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Add collection",
        name: "INV_NoCollectionUnlessAllChecksPass / INV_AllOrNone",
        axis: Tla,
        anchor: "crates/database/db/src/collection/ops/add/mod.rs DB::add_collection, prepare_new_collections, check_policies, check_collection_patch; crates/database/db/src/txn/registry/schemas.rs add_collection_in_txn_as; crates/database/db/src/txn/registry/views.rs DbTransactionRegistry::add_view_in_txn; crates/database/db/src/collection/ops/create.rs DB::create_collections_atomic, DB::stage_new_collections",
        model_ref: "MC_AddCollection_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Index changes in a transaction",
        name: "INV_WorkOnlyAfterCommit / INV_IsolatedUntilCommit / INV_SeesOwnChange / INV_BuiltWhenCommitReturns / INV_WorkSeesTxnWrites / INV_FailedChangeLeavesNothing",
        axis: Tla,
        anchor: "crates/database/db/src/index/in_txn.rs DB::create_index_in, DB::drop_index_in; crates/database/db/src/txn/registry/indexes.rs DbTransactionRegistry::create_index_in_txn, DbTransactionRegistry::drop_index_in_txn, DbTransactionRegistry::indexes_in_txn; crates/database/db/src/txn/registry/mod.rs finish_statement; crates/entry-points/defra-node/src/ops/index.rs Node::create_index_as, Node::delete_index_as, Node::list_indexes_as; crates/surfaces/http/src/handlers/index/go_api.rs go_create_index, go_list_indexes, go_delete_index; crates/database/db/src/collection/encrypted_index.rs DB::add_encrypted_index_in, DB::delete_encrypted_index_in, DB::encrypted_indexes_in, DB::all_encrypted_indexes_in; crates/database/db/src/txn/registry/indexes.rs DbTransactionRegistry::add_encrypted_index_in_txn, DbTransactionRegistry::delete_encrypted_index_in_txn, DbTransactionRegistry::encrypted_indexes_in_txn, DbTransactionRegistry::all_encrypted_indexes_in_txn; crates/entry-points/cli/src/encrypted_index_adapter.rs EncryptedIndexAdapter; crates/database/db/src/collection/ops/create.rs DB::stage_new_collections; crates/database/db/src/txn/registry/views.rs DbTransactionRegistry::add_view_in_txn, DbTransactionRegistry::refresh_views_in_txn",
        model_ref: "MC_IndexInTxn_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Encrypted index changes outside a transaction",
        name: "INV_NoLostIndex / INV_NoValidRefused",
        axis: Tla,
        anchor: "crates/database/db/src/collection/encrypted_index.rs DB::add_encrypted_index, DB::delete_encrypted_index, DB::rewrite_definition_in, definition_in; crates/database/db/src/txn/attempt.rs DB::transact_db; crates/entry-points/defra-node/src/ops/encrypted_index.rs NodeOps::add_encrypted_index, NodeOps::delete_encrypted_index; crates/entry-points/cli/src/encrypted_index_adapter.rs EncryptedIndexAdapter",
        model_ref: "MC_EncryptedIndexAutocommit_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Collection truncate",
        name: "INV_NoRemovalWithoutPermission / INV_NoRemovalFromChunks / INV_FilteredNeverOnBranchable / INV_FilteredRemovesOnlyCommittedMatches / INV_BadFilterRemovesNothing / INV_UnfilteredRemovesAll / INV_UnfilteredEmptiesView / INV_UnfilteredClearsSearchRecords",
        axis: Tla,
        anchor: "crates/database/db/src/collection/ops/truncate/mod.rs DB::truncate_collection_as, DB::truncate_parsed_as, DB::truncate_target, conditions, DB::truncate; crates/database/db/src/collection/ops/truncate/all.rs DB::truncate_all, DB::truncate_metadata_in; crates/database/db/src/collection/ops/truncate/filtered.rs DB::truncate_matching; crates/database/db/src/chunk/user_writes.rs ensure_admin_changeable; crates/database/db/src/merge/se/serve.rs store_artifacts, build_reply",
        model_ref: "MC_Truncate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Truncating a collection while an index is created or its build
        // fails: the cleanup reads the definition in its own transaction,
        // after the chunks, and deletes every declared ordered index's own
        // prefix, so no entry outlives the documents it named and no other
        // collection's keys go with them.
        family: "Collection truncate",
        name: "INV_NoEntryForTruncatedDoc / INV_OtherCollectionIntact / LIVE_TruncateFinishes",
        axis: Tla,
        anchor: "crates/database/db/src/collection/ops/truncate/all.rs DB::truncate_all, DB::truncate_metadata_in; crates/database/db/src/definition/loader.rs get_collections_by_collection_id; crates/database/db/src/index/create.rs DB::stage_index_definition; crates/database/db/src/index/backfill.rs BackfillPage::index, DB::record_backfill_failure",
        model_ref: "MC_TruncateIndexCleanup_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The key prefix the truncate deletes per index covers exactly that
        // index's entries: never another index's, and never a key of a
        // `/tag/name/` family, whatever the short ID. The collection-wide
        // prefix it replaced covered every document for short ID 100.
        family: "Collection truncate",
        name: "IndexPrefix.index_prefix_takes_its_entries / index_prefix_only_its_index / index_prefix_misses_family / old_prefix_of_100_covers_documents / old_prefix_of_118_covers_versions",
        axis: Lean,
        anchor: "crates/engine/storage/src/keys/utils/mod.rs encode_uvarint_ascending; crates/engine/storage/src/keys/datastore/index_key.rs IndexDataStoreKey::index_prefix; crates/engine/storage/src/keys/document.rs doc_key, deleted_doc_key; crates/database/db/src/collection/ops/truncate/all.rs DB::truncate_metadata_in",
        model_ref: "IndexPrefix (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // An unfiltered truncate deletes its collection's whole `/se/`
        // prefix: no search record of it survives, held or received, and no
        // other collection's record goes with it.
        family: "Collection truncate",
        name: "SeTruncatePrefix.none_left / others_kept / held_only_misses_received",
        axis: Lean,
        anchor: "crates/database/db/src/collection/ops/truncate/all.rs DB::truncate_metadata_in; crates/database/db/src/merge/se/serve.rs store_artifacts",
        model_ref: "SeTruncatePrefix (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Relation-filtered mutation",
        name: "INV_DecisionHoldsAtCommit / INV_AbortOnlyOnRangeWrite",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/mutation_inputs.rs resolve_filter_to_doc_ids; crates/engine/datastore/src/multistore.rs SharedTxn::set_scans_validated, SharedTxn::iterator; crates/database/db/src/read/doc.rs DbDocFetcher::set_scans_validated",
        model_ref: "MC_RelationFilterScan_{Green,Red_Unvalidated}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Add DAC policy",
        name: "INV_NoPolicyWithoutPermission / INV_NoPolicyWithoutCreator / INV_NoInvalidPolicy / INV_NoPolicyWithoutAcp / INV_ReturnedIsKeptLocally / INV_NoSilentFailure / INV_GoOrder / INV_SeedHonoured",
        axis: Tla,
        anchor: "crates/database/db/src/access/dac/mod.rs DB::add_dac_policy_as, DB::add_dac_policy_with_counter_as, DB::add_dac_policy_with_nonce_as, DB::add_policy, parse, build; crates/database/db/src/access/dac/policies.rs DacPolicies::keep, DacPolicies::get; crates/engine/acp/src/policy_lookup.rs RemotePolicies::add_policy",
        model_ref: "MC_DacPolicyAdd_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "vera.rs policy creation",
        name: "INV_OneRequestOnePolicy / INV_ReturnedIsOwn / INV_NoRefusalOfOwn / INV_NoForeignBlock / INV_SuppliedIdIsCallers",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/provider/create_policy.rs VeraRsProvider::create_policy_with, VeraRsProvider::adopt_existing; crates/engine/vera/src/vera_rs/provider/submission.rs VeraRsProvider::confirm_tx; crates/engine/vera/src/cosmos/dac.rs VeraDocumentACP::add_policy",
        model_ref: "MC_VeraPolicyCreate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Policy nonce text",
        name: "decode_encode / encode_length / encode_digits / parse_encode / encode_injective / decode_length / parse_length",
        axis: Lean,
        anchor: "crates/engine/acp/src/policy_nonce.rs PolicyNonce",
        model_ref: "VeraPolicyNonce (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Node access control operations",
        name: "INV_NoStatusWithoutPermission / INV_NoDisableWithoutPermission / INV_NoReEnableWithoutStoredPermission / INV_NoGrantUnlessAllChecksPass / INV_NoChangeUnlessConfigured / INV_RestartKeepsStatus / INV_NoSilentNoOp / INV_GoOrder / INV_RefusedWhenGoRefuses",
        axis: Tla,
        anchor: "crates/database/db/src/access/nac_ops/mod.rs DB::nac_status_as, DB::disable_nac_as, DB::re_enable_nac_as, DB::add_nac_actor_relationship_as, DB::delete_nac_actor_relationship_as, DB::change_nac_relationship; crates/database/db/src/nac/toggle.rs NacManager::decide_toggle, NacManager::require; crates/database/db/src/nac/write.rs NacManager::decide_relationship, NacTarget::parse; crates/database/db/src/nac/mod.rs NacManager::initialize; crates/engine/acp/src/nac/node_acp/operations.rs NodeACP::authorize_change",
        model_ref: "MC_NodeAcpOps_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/legacy/nac/core_operations.rs nac_core_operations_gate_test; crates/database/db/tests/nac/tests.rs test_nac_manager_disable_reenable, test_nac_manager_permission_check, an_empty_target_is_refused_and_never_reaches_the_wildcard",
    },
    Property {
        // A NAC relationship change refuses an empty target with Go's
        // ErrRelationshipTargetActorEmpty, after the identity and before the
        // relation, on add and delete alike, so it never reaches `*`.
        family: "Node access control operations",
        name: "empty_target_never_changes / empty_target_is_refused_as_go / identity_is_checked_before_the_target / delete_of_a_non_did_finds_nothing",
        axis: Lean,
        anchor: "crates/database/db/src/nac/write.rs NacManager::decide_relationship, NacTarget::parse; crates/database/db/src/access/nac_ops/mod.rs DB::change_nac_relationship",
        model_ref: "NacRelationship (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Node access control operations",
        name: "INV_RaceNoChangeWithoutPermission / INV_RaceOneWinner / INV_RaceLoserHearsAlready",
        axis: Tla,
        anchor: "crates/database/db/src/nac/toggle.rs NacManager::toggle_as, NacManager::decide_toggle; crates/database/db/src/nac/write.rs NacManager::in_write, NacManager::decide_relationship; crates/engine/acp/src/zanzibar/store/persistent.rs PersistentZanzibarStore::transact; crates/database/db/tests/access/nac_toggle_race.rs",
        model_ref: "MC_NodeAcpOps_{Race_Green,Red_SplitScope,Red_OutsideGuard,Red_NoRetry}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/access/nac_toggle_race.rs of_two_disables_one_wins_and_the_other_is_already_disabled",
    },
    Property {
        family: "Node access control operations",
        name: "INV_RelNoChangeWhileOff / INV_RelOffHearsGo",
        axis: Tla,
        anchor: "crates/database/db/src/nac/write.rs NacManager::in_write, NacManager::decide_relationship; crates/database/db/src/access/nac_ops/mod.rs DB::change_nac_relationship; crates/database/db/tests/access/nac_toggle_race.rs",
        model_ref: "MC_NodeAcpOps_{Rel_Green,Red_RelOutsideGuard}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/access/nac_toggle_race.rs a_disable_that_commits_first_refuses_the_relationship_with_gos_error",
    },
    Property {
        family: "Node access control operations",
        name: "disable_turns_off / re_enable_turns_on / re_enable_undoes_disable / toggles_keep_grants / writes_only_when_allowed / writes_only_when_configured / disable_writes_only_from_on / re_enable_writes_only_from_off / re_enable_checks_the_caller_first / rerun_after_a_win_hears_already / enable_writes_only_when_not_configured / enable_refuses_a_disabled_nac / enable_leaves_a_disabled_store / startup_keeps_a_disabled_nac_off",
        axis: Lean,
        anchor: "crates/engine/acp/src/nac/node_acp/mod.rs NodeACP::load; crates/engine/acp/src/nac/node_acp/lifecycle.rs NodeACP::disable, NodeACP::re_enable, NodeACP::enable; crates/database/db/src/nac/toggle.rs NacManager::decide_toggle; crates/database/db/src/nac/mod.rs NacManager::initialize",
        model_ref: "NacToggle (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The C API's options structs grow only at their end and default to
        // zero: the library reads the caller's bytes up to the caller's size,
        // treats the rest as zero, and refuses a set field it does not know.
        family: "FFI options structs",
        name: "keeps_what_the_caller_gave / older_caller_gets_defaults / refuses_an_unknown_set_field / refuses_a_length_inside_a_field / unset_new_fields_read_as_absent / unset_new_fields_are_accepted",
        axis: Lean,
        anchor: "crates/entry-points/ffi/src/options.rs read, ZeroDefault",
        model_ref: "FfiOptions (lake build)",
        tiers: &[Contract],
        bound_by: "crates/entry-points/ffi/src/options_tests.rs null_options_are_every_default, an_older_callers_shorter_struct_gets_defaults_for_the_new_fields, a_newer_callers_longer_struct_is_read_when_its_new_fields_are_unset, a_set_field_the_library_does_not_know_is_refused, a_size_that_stops_inside_a_field_is_refused; crates/entry-points/ffi/src/api/sized_tests.rs options_this_library_does_not_know_are_refused, an_older_callers_shorter_options_get_defaults_for_the_rest, a_size_that_cuts_a_field_is_refused",
    },
    Property {
        // A document write or a truncate checks `get-collection` and then
        // its own permission, in Go's order, inside its transaction, and a
        // call that names no identity acts as no one, never as the node.
        // Over HTTP the edge checks first, in the same order, from a
        // snapshot of the grants swapped in before any NAC write returns.
        family: "Node access control operations",
        name: "INV_NoEffectWithoutPermission / INV_RefusalNamesGoFirst / INV_EdgeSeesReturnedRevoke",
        axis: Tla,
        anchor: "crates/engine/acp/src/nac/permission.rs NodePermission::go_order; crates/database/db/src/access/nac.rs check_node_access_as, check_node_access_in_txn; crates/database/db/src/write/autocommit/batch.rs check_attempt_node_access; crates/database/db/src/txn/registry/indexes.rs check_access_in; crates/engine/query/src/runner/turn.rs lent, lent_node_permission; crates/entry-points/defra-node/src/ops/request.rs caller, txn_owner; crates/surfaces/http/src/auth_middleware.rs auth_middleware; crates/surfaces/http/src/route_permissions.rs reads_collection_first, route_permission; crates/surfaces/http/src/nac_guard.rs require_permissions; crates/surfaces/http/src/nac_adapter.rs NacAdapter::check_permissions; crates/database/db/src/nac/mod.rs NacManager::check_permissions; crates/database/db/src/nac/edge.rs refresh_edge, refresh_edge_on_commit, refresh_guard, RefreshGuard, reload_after_commit, EdgeGrants::publish, EdgeSnapshot::check, check_all, decide; crates/engine/acp/src/nac/node_acp/snapshot.rs copy_into; crates/database/db/src/nac/write.rs in_write; crates/database/db/src/nac/mod.rs initialize, enable, purge; crates/entry-points/defra-node/src/node_builder.rs NodeBuilder::build",
        model_ref: "MC_NacOrder_{Green,Red_SkipGet,Red_OwnFirst,Red_AnonAsNode,Red_OutsideTxn,Red_EdgeOwnOnly,Red_StaleEdge,Red_NoWithdraw}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/cli/src/nac_adapter_tests.rs a_returned_revocation_refuses_the_next_edge_check, the_snapshot_decides_every_permission_as_the_engine_does; crates/database/db/tests/nac/dropped_write.rs a_revoke_dropped_after_its_commit_still_checks_the_edge",
    },
    Property {
        // A database transaction that commits a NAC tuple write (through a
        // projector or a source grant) rebuilds the edge snapshot at commit;
        // one that writes none registers nothing and rebuilds nothing. A
        // savepoint rolled back drops the registrations made after it.
        family: "Node access control operations",
        name: "INV_RebuiltAfterNacWrite / INV_NoRebuildWithoutNacWrite",
        axis: Tla,
        anchor: "crates/engine/acp/src/zanzibar/store/transaction.rs written; crates/engine/datastore/src/multistore.rs node_access_written, NodeAccessWrites; crates/database/db/src/nac/edge.rs EdgeRefreshOwed, refresh_edge_on_commit; crates/database/db/src/database/mod.rs new_db_txn; crates/engine/storage/src/corekv/callbacks.rs TxnCallbacks::mark, TxnCallbacks::truncate",
        model_ref: "MC_NacEdgeRefreshOnWrite_{Green,Red_EveryWrite,Red_KeepOnRollback}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/acp/tests/node_access_writes.rs only_node_policy_writes_owe_the_edge_refresh; crates/entry-points/defra-node/tests/unit/document_projector.rs raw_document_mutations_stage_sources_in_the_same_transaction",
    },
    Property {
        family: "Node access control operations",
        name: "goOrder_collection_write / goOrder_alone / firstMissing_none_iff / firstMissing_some / allowed_needs_both / refused_get_collection_first / no_bypass_when_off / no_bypass_anonymous / no_bypass_when_denied / bypass_iff_held",
        axis: Lean,
        anchor: "crates/engine/acp/src/nac/permission.rs NodePermission::go_order; crates/database/db/src/access/nac.rs check_node_access_as, check_node_access_in_txn; crates/engine/query/src/runner/turn.rs lent, lent_node_permission; crates/database/db/src/read/lensed/fetcher/mod.rs dac_bypass; crates/foundation/defra-core/src/request_context.rs RequestContext::with_dac_bypass_denied; crates/surfaces/p2p-adapter/src/libp2p.rs P2PAdapter::check_nac; crates/surfaces/p2p-adapter/src/iroh.rs IrohP2PAdapter::check_nac",
        model_ref: "NacOrder (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A read whose bypass the engine resolves bypasses document access
        // control exactly when node access control is on and grants the
        // caller `bypass-dac`, decided at the read's own snapshot, and its
        // caller did not deny it the bypass. A bypass the caller set on the
        // request (`RequestContext::with_dac_bypass`) is honoured as set,
        // without the engine consulting node access control.
        family: "Node access control operations",
        name: "INV_BypassOnlyWhenGranted / INV_BypassWhenGranted / INV_DeniedNeverBypasses",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/turn.rs lent; crates/database/db/src/read/lensed/fetcher/mod.rs dac_bypass; crates/database/db/src/collection/acp.rs check_doc_permission_as; crates/foundation/defra-core/src/request_context.rs RequestContext::with_dac_bypass_denied, RequestContext::with_dac_bypass",
        model_ref: "MC_NacDacBypass_{Green,Red_NacOff,Red_Never,Red_Live,Red_IgnoreDenial}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh solicited replies",
        name: "INV_NoUnsolicitedAdmitted / INV_SolicitedAdmitted / INV_NoReplyStreamOnV2",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/solicited.rs Solicited::expect, Solicited::take, Solicited::prune; crates/database/p2p/src/iroh/endpoint_streams.rs admit_stream, dispatch_stream; crates/database/p2p/src/iroh/endpoint_commands.rs handle_command; crates/database/p2p/src/iroh/endpoint_rpc.rs handle_reply_on_stream",
        model_ref: "MC_SolicitedReplies_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Reply expectations are keyed by (peer, tag) across all of the peer's
        // connections, and are taken before the size, budget and body checks.
        // A concurrent mux 0.1 connection can therefore open up to that many
        // unowed reply streams (still budgeted and matched by message id), and
        // a reply reset for room or size loses its expectation.
        family: "Iroh solicited replies",
        name: "INV_NoUnsolicitedAdmitted counts expectations per (peer, tag), not per connection or reply",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/solicited.rs Solicited::expect, Solicited::take; crates/database/p2p/src/iroh/endpoint_streams.rs admit_stream",
        model_ref: "MC_SolicitedReplies_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh service records",
        name: "validate_sound / validate_complete / offer_never_lowers / offer_replaces_expired / offer_keeps_only_valid / live_not_expired / encodeKey_injective / no_secret_not_advertised / distinct_ids_distinct_keys / parse_print / providers_exclude_self / providers_nodup / providers_from_book / providers_within_cap / providers_sorted",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/records/verify.rs verify; crates/database/p2p/src/iroh/records/book.rs Book::offer, Book::live; crates/database/p2p/src/iroh/records/key.rs blind, advertised_key, CollectionKey::encode; crates/database/p2p/src/iroh/records/pointer.rs Pointer::print, Pointer::parse; crates/database/p2p/src/iroh/records/task.rs Records::providers, rank",
        model_ref: "ServiceRecord (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Lean models chunking under any close rule that reads the item and
        // its open chunk, the byte cap that bounds the open chunk
        // (`open_bounded`), `build` over a sorted list, the one-pass
        // `Builder`, the windowed edit level by level and walked up to the
        // root with the single-child `collapse`, `diff`'s walk against a
        // per-key comparison, the injective node encoding, and its decoder,
        // which reads a node's bytes back and refuses bytes after them. SHA-256 is
        // trusted collision-free; an edit's window is any run of chunks that
        // holds the change, where Rust finds it by key.
        family: "Prolly tree catch-up",
        name: "chunks_append / closes_append / chunks_flatten / chunks_nonempty / chunks_shrink / open_bounded / build / sorted_insert / sorted_erase / eq_of_same_entries / build_same_entries / insert_comm / erase_insert_comm / insert_insert / canon_chunks / rewrite_level_refines / rewrite_level_up / walk_levels / edit_root / builder_root / diff_eq / mergeDiff_spec / encode_injective / decode_encode / decode_refuses_trailing",
        axis: Lean,
        anchor: "crates/foundation/prolly/src/chunk.rs Chunker::push, Chunker::finish, chunk; crates/foundation/prolly/src/hash.rs is_boundary; crates/foundation/prolly/src/tree.rs Tree::build, raise, collapse; crates/foundation/prolly/src/build.rs Builder::push, Builder::finish, Builder::feed, Builder::close; crates/foundation/prolly/src/edit.rs Tree::edit, rewrite_level, splice; crates/foundation/prolly/src/diff.rs diff, Walk::open, Walk::take; crates/foundation/prolly/src/node.rs Node::encode, Node::decode, Item::encoded_len, MAX_CHUNK_BYTES",
        model_ref: "ProllyTree (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The pull is not wired into p2p yet (#2977); the model is its contract.
        // Each pull is one atomic step and read access never changes, so
        // "caught up" means caught up with the responder as it was at that
        // step, and a read revoked mid-pull is not checked. PulledHolds is
        // about pulls whose walk completes: `ProllyTree.diff_eq` and
        // `ProllyTree.mergeDiff_spec` show a completed walk reports exactly the
        // differing keys, but not that the walk ends or that every node read
        // succeeds. A walk that fails makes no "caught up" claim and changes
        // nothing, a stutter step the model already allows.
        family: "Prolly tree catch-up",
        name: "NoLeak / PulledHolds: a pull never shows a peer a document it may not read, and a pull that ends caught up leaves the puller holding every version the responder could show it at that pull",
        axis: Tla,
        anchor: "crates/foundation/prolly/src/tree.rs Tree::build, Tree::root; crates/foundation/prolly/src/diff.rs diff",
        model_ref: "MC_ProllySync_Green.cfg (GREEN) / MC_ProllySync_{Red_NoFilter,Red_StaleRoot,Red_MissingOnly,Red_KeepClaim}.cfg (RED)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Node contents are not modelled: a node is named by its hash, so a
        // cached or pending copy is never stale. One worker owns the store,
        // so a flush and a read never overlap. The Rust cache's CLOCK victim
        // choice is one of the model's "any part that fits".
        family: "Prolly tree node store and cache",
        name: "GetFinds / CacheBounded / HugeEvictsNothing / PublishedStored / BuildWhole: a node put is always readable, the cache stays under its byte budget, a node over budget evicts nothing, a published root names only stored nodes, and a build whose put failed never returns a tree",
        axis: Tla,
        anchor: "crates/foundation/prolly/src/cache.rs Cached::get, Cached::insert; crates/foundation/prolly/src/tree.rs collapse; crates/engine/storage/src/stores/prolly_nodes.rs ProllyNodes::get, ProllyNodes::put, ProllyNodes::flush; crates/foundation/prolly/src/build.rs Builder::push, Builder::finish",
        model_ref: "MC_ProllyNodeCache_Green.cfg (GREEN) / MC_ProllyNodeCache_{Red_SkipPending,Red_NoEvict,Red_CacheHuge,Red_DropOnFail,Red_PublishBeforeFlush,Red_FinishAfterFail}.cfg (RED)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "INV_ListMatchesText / INV_ListedChunksExist / INV_NoChunkOfDeletedParent / INV_ValidWritesSucceed",
        axis: Tla,
        anchor: "crates/database/db/src/chunk/sync/mod.rs set_lists, ChunkWrites::create_missing, ChunkWrites::delete_all; crates/database/db/src/chunk/sync/lookup.rs ChunkCollection::existing_of; crates/database/db/src/write/autocommit/one_op.rs AutoCommitMutator::in_batch; crates/database/db/src/write/autocommit/batch.rs BatchMutator::update_as, delete_as; crates/database/db/src/write/doc.rs DbDocMutator; crates/database/db/src/write/prepare.rs prepare_request_write",
        model_ref: "MC_ParentChunks_Green.cfg / MC_ParentChunks_Red_SeparateTxn.cfg / MC_ParentChunks_Red_NoCascade.cfg / MC_ParentChunks_Red_NoValidation.cfg / MC_ParentChunks_Red_CreateWithoutList.cfg / MC_ParentChunks_Red_NoLookup.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "INV_ChunksFollowParent / INV_NoForeignChunks / INV_DeletesFollowParent / INV_SyncSendsCurrentChunks",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/broadcast.rs SyncCoordinator::push_jobs, SyncCoordinator::push_followed_document_to_replicators_with_creator; crates/database/db/src/merge/push_docs/follow.rs replica_takes; crates/database/db/src/chunk/sync/write.rs ChunkWrites::publish; crates/database/db/src/merge/broadcast_mutator/mod.rs BroadcastMutator::derived_heads; crates/database/p2p/src/sync/doc_sync_ids.rs with_followers, check_doc_id_limit; crates/database/p2p/src/sync/coordinator/event_handler/doc_sync.rs SyncCoordinator::handle_doc_sync_request; crates/database/p2p/src/sync/coordinator/pubsub_services.rs DocSyncHandler::handle; crates/database/db/src/merge/head_provider.rs DbHeadProvider::following_doc_ids; crates/database/db/src/chunk/follow.rs current_chunk_ids; crates/database/db/src/chunk/sync/lookup.rs ChunkCollection::chunks_with",
        model_ref: "MC_ChunkFollow_Green.cfg / MC_ChunkFollow_Red_OwnFilter.cfg / MC_ChunkFollow_Red_AllChunks.cfg / MC_ChunkFollow_Red_SyncParentOnly.cfg / MC_ChunkFollow_Red_SyncHistory.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/chunk/replicate_filtered.rs a_filtered_replicator_ships_the_matching_parents_chunks; tools/integration-test/tests/testo/chunk/replicate_doc_sync.rs a_document_sync_brings_the_parents_chunks",
    },
    Property {
        family: "Text chunking",
        name: "covers / bounded / pickPre_capped / deterministic / units_edit / local_edit / heading_keeps_text / unit_break_reads_only_headings / long_unit_fences",
        axis: Lean,
        anchor: "crates/building-blocks/chunk-split/src/split/mod.rs split, split_v1; version.rs SplitterVersion; segments.rs units, fence_before, segments; atoms.rs atoms, sentence_ends; pack.rs pack; overlap.rs with_overlap, borrow, REPEAT_SHARE",
        model_ref: "ChunkSplit (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "no_chunks_keeps_its_identity / different_settings_different_identity / field_order_does_not_change_identity / peer_holds_the_same_settings_as_the_author / buggy_identity_ignores_settings / decode_encode / default_splitter_keeps_the_old_bytes / old_bytes_read_as_the_default / encode_injective / splitter_reaches_the_identity",
        axis: Lean,
        anchor: "crates/foundation/schema/src/cid.rs build_collection_delta; crates/engine/query/src/sdl_parse/chunk.rs synthesize_chunk_types; crates/database/db/src/merge/merge_handler/definition.rs process_collection_definition_delta; crates/foundation/defra-core/src/block_delta.rs ChunkedFieldDelta, DEFAULT_SPLITTER_VERSION, is_default_splitter_version; crates/foundation/defra-core/src/ipld/to_ipld.rs; crates/foundation/defra-core/src/ipld/from_ipld.rs",
        model_ref: "ChunkSplit (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "INV_ReadsMatchText / INV_SettledPeersAgree / INV_NoForgedChunk / INV_ChunksNeverChange / INV_ListedChunksStay / INV_DeletesNotLost / INV_TextChunksStored / INV_OwedIsEncrypted / INV_SettledListsAgree / INV_EachChunkOnce",
        axis: Tla,
        anchor: "crates/engine/query/src/plan/chunk_parent.rs parent_hashes; crates/building-blocks/chunk-split/src/list.rs chunk_list; crates/building-blocks/crdt/src/field/lww.rs merge; crates/database/db/src/chunk/guard.rs check_chunk_merge; crates/database/db/src/chunk/peer_delete.rs delete_must_wait; crates/database/db/src/merge/merge_handler/composite_persist.rs; crates/database/db/src/merge/merge_handler/doc_identity.rs resolve_composite_doc_id_in_txn; crates/database/db/src/merge/merge_handler/composite.rs process_composite_delta_locked; crates/database/db/src/chunk/rebuild.rs build_missing; crates/database/db/src/chunk/sync/write.rs ChunkWrites::create; crates/database/db/src/chunk/follow.rs current_chunk_ids",
        model_ref: "MC_ChunkMerge_Green.cfg / MC_ChunkMerge_Green_Delete.cfg / MC_ChunkMerge_Green_Encrypted.cfg / MC_ChunkMerge_Red_TrustList.cfg / MC_ChunkMerge_Red_NoHashCheck.cfg / MC_ChunkMerge_Red_MutableChunk.cfg / MC_ChunkMerge_Red_NoDeleteGuard.cfg / MC_ChunkMerge_Red_ListedDeleteOnly.cfg / MC_ChunkMerge_Red_RefuseDelete.cfg / MC_ChunkMerge_Red_UnstoredDelete.cfg / MC_ChunkMerge_Red_NoRebuild.cfg / MC_ChunkMerge_Red_SignedChunkIds.cfg / MC_ChunkMerge_Red_EncryptedNotShipped.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "INV_ShownIsLiveAndReadable / INV_ShownIsTheText / INV_ShownPassesOwnApp / INV_ServedIsReadable / INV_DelegatedKeyInScope",
        axis: Tla,
        anchor: "crates/engine/query/src/plan/chunk_parent.rs parent_hashes, verified_hash; crates/engine/query/src/plan/chunk_gate.rs ChunkGate::allows, ChunkGate::decide; crates/engine/query/src/plan/permission_filter.rs permission_check; crates/database/db/src/chunk/access.rs history_parents, read_owners, parent_collection; crates/engine/query/src/runner/chunk_access.rs chunk_readable, ChunkReads; crates/engine/query/src/planner/builder/mod.rs maybe_wrap_with_acp_filter; crates/surfaces/p2p-adapter/src/read_gate.rs DbBlockReadGate::may_read, DbBlockClassifier::classify; crates/database/db/src/block/verify.rs load_authorized_block_signature; crates/surfaces/http/src/handlers/events.rs can_observe_document_event; crates/database/kms/src/defra_kms.rs DefraKms::local_release_decision; crates/database/kms/src/nac_dac_policy.rs Delegation::covers",
        model_ref: "MC_ChunkRead_Green.cfg / MC_ChunkRead_Red_NoDeletedCheck.cfg / MC_ChunkRead_Red_NoHashCheck.cfg / MC_ChunkRead_Red_SharedMemo.cfg / MC_ChunkRead_Red_NoMergedCheck.cfg / MC_ChunkRead_Red_NoGrantCheck.cfg / MC_ChunkRead_Red_NoListCheck.cfg / MC_ChunkRead_Red_NoResolveCheck.cfg / MC_ChunkRead_Red_TrustList.cfg / MC_ChunkRead_Red_NoOwnAppCheck.cfg / MC_ChunkRead_Red_DelegationScope.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "INV_NoSilentOverwrite / INV_LosesOnlyToItsFields",
        axis: Tla,
        anchor: "crates/database/db/src/write/autocommit/selected.rs dependencies, unchanged; crates/database/db/src/write/autocommit/update.rs run_update_attempt; crates/database/db/src/write/autocommit/one_op.rs UpdateWork::run",
        model_ref: "MC_SelectedUpdate_Green.cfg / MC_SelectedUpdate_Red_ExpectedOnly.cfg / MC_SelectedUpdate_Red_Whole.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "no_orphan_chunks / parent_is_gone / others_stay / delete_alone_leaves_an_orphan",
        axis: Lean,
        anchor: "crates/database/db/src/collection/ops/delete.rs delete_collection_with_txn; crates/database/db/src/collection/ops/chunk_versions.rs chunk_versions_to_add",
        model_ref: "ChunkSplit (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "INV_LiveParentHasChunks / INV_PurgedHaveNoChunks",
        axis: Tla,
        anchor: "crates/database/db/src/collection/ops/truncate/all.rs truncate_all; crates/database/db/src/collection/ops/purge.rs purge_documents",
        model_ref: "MC_TruncateChunks_Green.cfg / MC_TruncateChunks_Red_ParentsFirst.cfg / MC_TruncateChunks_Red_PurgeLeavesChunks.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "INV_CommittedHeadOwned / LIVE_AllDelivered",
        axis: Tla,
        anchor: "crates/database/db/src/merge/broadcast_mutator/heads.rs register_committed_heads; crates/database/db/src/merge/broadcast_mutator/mod.rs BroadcastMutator::register_write_heads, BroadcastMutator::derived_heads; crates/database/db/src/merge/broadcast_mutator/batch.rs BroadcastBatchMutator::register_pending",
        model_ref: "MC_ChunkMarkerHandoff_Green.cfg / MC_ChunkMarkerHandoff_Red_RequestOwned.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/merge/broadcast_marker_handoff.rs a_create_registers_every_chunk_after_its_caller_stops_waiting, a_batch_registers_every_chunk_after_its_caller_stops_waiting",
    },
    Property {
        // A peer's definition blocks arrive in any order and may be forged.
        // A first version follows the SDL's rules, a patch keeps its previous
        // version's settings, and activation refuses settings other than the
        // first version's, even for a record stored before the merge checked.
        family: "Text chunking",
        name: "FirstValid / MergedMatchFirst / ActiveMatchesFirst / MergedKeepName / ActiveKeepsName: a synced version's @chunk settings are ones a schema could declare, and every merged or active version keeps its collection's first version's settings and name",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/definition.rs process_collection_definition_delta; crates/database/db/src/merge/merge_handler/definition_chunks.rs chunk_settings_error, chunk_parent_error; crates/foundation/schema/src/chunk_rules.rs chunk_settings_errors, chunk_settings_change, ensure_chunk_settings_kept; crates/database/db/src/chunk/user_writes.rs ensure_keeps_first_chunk_settings; crates/database/db/src/collection/ops/version.rs set_active_collection_version_as; crates/database/db/src/txn/registry/collections.rs set_collection_active_in_txn_as",
        model_ref: "MC_ChunkSettingsSync_{Green,Green_Legacy}.cfg (GREEN) / MC_ChunkSettingsSync_{Red_NoFirstCheck,Red_NoMergeGuard,Red_NoActivateGuard,Red_NoNameCheck}.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/merge/chunk_definition_guard.rs a_patch_that_changes_max_chars_is_rejected, a_first_version_with_max_chars_below_the_minimum_is_rejected, an_honest_patch_that_keeps_the_chunk_settings_is_accepted, activating_a_version_with_other_chunk_settings_is_refused, a_patch_that_changes_the_splitter_version_is_rejected, activating_a_version_with_another_splitter_version_is_refused, a_patch_that_renames_the_parent_is_rejected, activating_a_renamed_version_is_refused; crates/foundation/schema/tests/chunk_validation_tests.rs renaming_the_parent_is_refused, renaming_the_chunk_collection_is_refused",
    },
    Property {
        // One task serves a pg-compat connection: one deadline read at the
        // accept bounds the peek, the SSL and GSS refusals and the startup;
        // an extended-protocol error skips to the Sync; a fatal error or a
        // cancel request closes it and the loop returns. A copy's CopyDone
        // leaves the copy, so an Execute copy's Sync reaches on_sync.
        family: "Postgres wire connection loop",
        name: "INV_StartupWithinDeadline / INV_StartedNeverTimedOut / INV_NoQueryBeforeStartup / INV_SkipUntilSync / INV_SyncResumes / INV_CopyEnds / INV_NothingAfterEnd / LIVE_Ends: no connection outlives the startup deadline unstarted, nothing but Sync is handled during a skip, nothing is read after the end",
        axis: Tla,
        anchor: "crates/database/pg-compat/src/wire/process.rs serve, run, refuse_ssl, dispatch, report; crates/database/pg-compat/src/wire/connection.rs Connection::next, Connection::is_closed, Connection::poll_close; crates/database/pg-compat/src/handler/auth.rs DIDAuthHandler::on_startup",
        model_ref: "MC_PgWire_{Green_Startup,Green_Protocol,Red_PeekNoDeadline,Red_SslNoDeadline,Red_StartupNoDeadline,Red_FreshDeadline,Red_DeadlineAfterStartup,Red_NoAwaitingSync,Red_HandleWhileSkipping,Red_SyncKeepsSkipping,Red_CopyFlagFromMessage,Red_CopyDoneStays,Red_ExtendedCopyDoneStays,Red_LoopAfterClose,Red_LoopAfterCloseRuns,Red_LoopAfterCloseHangs,Red_FatalStaysOpen,Red_StartupErrorNotFatal}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/pg-compat/tests/wire.rs ssl_is_refused_then_queries_run_and_a_fatal_error_closes_the_connection, a_client_that_starts_tls_directly_is_closed, a_client_that_sends_nothing_is_closed_at_the_startup_deadline",
    },
    Property {
        family: "HTTP client redirects and body deadline",
        name: "credentials_stay_with_their_origin / at_most_limit_plus_one_exchanges / endless_redirects_fail / same_origin_redirect_keeps_credentials / cross_origin_redirect_strips_credentials: a credential from the request or the defaults reaches only the request's origin, a proxy credential only its own hop, within eleven exchanges",
        axis: Lean,
        anchor: "crates/building-blocks/net/src/http/client.rs Client::execute, Client::follow, Client::exchange, redirect, REDIRECT_LIMIT",
        model_ref: "HttpClient.Redirect (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/building-blocks/net/tests/client.rs redirects_are_followed_and_change_the_method_as_the_status_says, credentials_follow_a_redirect_only_to_the_same_origin",
    },
    Property {
        family: "HTTP client redirects and body deadline",
        name: "ends_after_timeout / at_most_one_timeout / deadline_wins_over_a_ready_piece / never_waits_past_the_deadline / no_timeout_before_the_deadline / no_limit_no_timeout: past the limit a body stream times out once, even with a piece ready, then gives its end and nothing more",
        axis: Lean,
        anchor: "crates/building-blocks/net/src/http/response.rs BytesStream::poll_next, Response::bytes_stream",
        model_ref: "HttpClient.BodyDeadline (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/building-blocks/net/tests/body_deadline.rs a_stalled_body_stream_gives_one_timeout_then_ends, every_chunk_read_past_the_limit_times_out_at_once, a_piece_ready_past_the_limit_gives_the_timeout",
    },
    Property {
        family: "Connect fallback race",
        name: "INV_OneSocket / INV_FallbackWaits / INV_NeverIdle / INV_ErrorOnlyWhenAllFail / INV_LastError / INV_WithinLimit / LIVE_Ends: one socket comes back, the other family starts after the delay or a failure, an error only once every family failed",
        axis: Tla,
        anchor: "crates/building-blocks/net/src/connect.rs first_connected, Connector::socket, FALLBACK_DELAY",
        model_ref: "MC_ConnectFallback_{Green,Green_NoTimeout,Green_Single,Red_NoDelay,Red_WaitAfterFailure,Red_FirstErrorWins,Red_KeepFirstError,Red_KeepLoser,Red_TimeoutMissesRace,Red_TimeoutMissesRaceHangs}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/building-blocks/net/tests/connect.rs the_other_family_connects_when_the_first_is_silent_past_the_delay, the_other_family_is_tried_when_the_first_refuses, when_both_families_refuse_the_last_refusal_comes_back",
    },
    Property {
        family: "Pubsub RPC reply addressing",
        name: "INV_NoAnswerLost / INV_AllAnswered / LIVE_Done / LIVE_AllArrive: a peer answers a request on its publisher's reply topic, even when a relay handed it the request, so no answer is lost and the asker's wait ends with every answer, or at its deadline while some answer is still missing or on its way",
        axis: Tla,
        anchor: "crates/database/p2p/src/host/p2p_host/protocols.rs handle_gossipsub_event; crates/database/p2p/src/sync/coordinator/event_handler/pubsub_raw.rs handle_gossip_raw_message; crates/database/p2p/src/pubsub_rpc/topic_handler.rs TopicHandler::deliver_gossip_message; crates/database/p2p/src/sync/coordinator/pubsub_client.rs pubsub_sync_branchable_collection; crates/database/p2p/src/iroh/gossip/delivery.rs raw_publisher",
        model_ref: "MC_PubsubRpcReply_{Green,Red_AnswerRelay,Red_DeadlineCutsOff}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/event_handler/mod.rs relayed_pubsub_rpc_request_is_answered_on_its_publishers_topic, relayed_kms_request_reaches_the_handler_as_its_publisher; crates/database/p2p/tests/pubsub_rpc_relayed_request.rs a_relayed_request_names_its_publisher; crates/database/p2p/tests/iroh/relayed_raw_request.rs a_relayed_raw_request_names_its_publisher; crates/database/p2p/src/iroh/gossip/delivery.rs a_sealed_raw_message_names_its_publisher_not_the_relay",
    },
    Property {
        family: "Pubsub RPC reply addressing",
        name: "open_seal / open_seal_never_unsigned / open_unsigned / open_refuses_short: an iroh raw payload sealed by its publisher opens to that publisher, is never taken for an unsealed one, and an unsealed one passes through untouched",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/gossip/raw_origin.rs seal, open; crates/database/p2p/src/iroh/gossip/delivery.rs raw_publisher",
        model_ref: "GossipFrame (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/iroh/gossip_raw_origin.rs sealed_payload_opens_to_its_origin, a_truncated_seal_is_refused_not_passed_through, an_unsealed_cbor_payload_passes_through",
    },
    Property {
        family: "ActivePeers listen addresses",
        name: "INV_ListenAddress / INV_NotHeldByLater / LIVE_Answered: ActivePeers never lists a peer that was waiting at the call at its source port while its Identify may still come, a peer that dials in after the call never holds it, and every call is answered",
        axis: Tla,
        anchor: "crates/database/p2p/src/host/command_handler/network.rs handle_peer_addresses, on_identify_settled, answer_peer_addresses; crates/database/p2p/src/host/p2p_host/swarm.rs handle_swarm_event, handle_connection_closed; crates/database/p2p/src/host/p2p_host/protocols.rs handle_identify_event",
        model_ref: "MC_ActivePeersIdentify_{Green,Red_AnswerAtOnce,Red_WaitOnLater}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/host/active_peers_tests.rs active_peers_lists_a_dialer_at_its_listen_address_before_its_identify, active_peers_is_not_held_by_a_peer_that_dials_in_after_the_call",
    },
    Property {
        family: "Gossip announcement audience",
        name: "INV_GoAudience / LIVE_Announced: a write committed with no peer subscribed is announced at once and never published again, so it reaches only peers subscribed at its commit, as Go's SendUpdate; every write is announced",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/broadcast.rs broadcast_local_update_with_creator; crates/database/p2p/src/sync/peer_state/tracker/operations.rs has_subscriber; crates/database/p2p/src/sync/broadcast_coalescer.rs BroadcastCoalescer::run; crates/database/db/src/merge/broadcast_mutator/broadcast.rs broadcast_with_creator",
        model_ref: "MC_AnnounceAudience_{Green,Red_HoldWhenAlone,Red_AloneMeansDisconnected,Red_RetryWhileConnected}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/broadcast_pool_tests.rs a_write_with_no_peer_connected_is_not_held_for_a_peer_that_joins, a_write_with_no_peer_subscribed_is_not_held_for_a_peer_that_subscribes; crates/database/db/tests/merge/broadcast_mutator_broadcast.rs a_write_that_finds_no_subscriber_is_not_published_again",
    },
    Property {
        // The first shutdown caller to swap the flag builds the one drain,
        // which owns the only completion sender; a closed edge hands the
        // drain back and the winner runs it in place. The drain waits for
        // pruners, takes the tracked set, and aborts what outlives its
        // deadline, so no tracked task keeps the coordinator alive. The
        // GREEN configurations assume prune_finished_tasks reads the flag
        // after announcing itself; Red_PrunerIgnoresFlag is the code without
        // that read.
        family: "Coordinator shutdown drain",
        name: "INV_NoLeak / INV_OneDrain / INV_ErrMeansCancelled / LIVE_CallersReturn: one drain runs, no tracked task is left running in the set once it returns, every shutdown caller returns",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs SyncShutdownHandle::shutdown, begin_shutdown, drain_background_tasks, spawn_task, spawn_here, register_task, prune_finished_tasks; crates/database/p2p/src/network.rs Network::try_spawn; crates/foundation/edge/src/handle.rs Edge::try_spawn",
        model_ref: "MC_CoordinatorShutdown_{Green,Green_TwoSpawns,Red_DropRefused,Red_CheckThenSpawn,Red_NoAbort,Red_SenderOutsideDrain,Red_TwoWinners,Red_NoPrunerWait,Red_PrunerIgnoresFlag,Red_NoRecheck}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/coordinator/shutdown.rs concurrent_shutdown_callers_wait_for_task_completion, shutdown_rejects_new_tasks_without_spawning_them, task_registration_racing_shutdown_does_not_leave_resources_alive, shutdown_on_a_closed_edge_runs_the_drain_and_releases_tracked_tasks",
    },
    Property {
        family: "Per-peer stream cap",
        name: "INV_WithinCap / INV_SlotsFollowStreams / INV_ForgottenAreIdle",
        axis: Tla,
        anchor: "crates/database/p2p/src/two_stream/peer_streams.rs PeerStreams::admit, StreamAdmission; crates/database/p2p/src/two_stream/runner.rs TwoStreamRunner::run",
        model_ref: "MC_PeerStreamCap_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Replies to this node's own requests arrive as inbound streams and
        // share the per-peer cap and the max_p2p_tasks pool with requests, as
        // in Go's rcmgr. A reply past the cap is reset and shows up as a reply
        // timeout. libp2p only.
        family: "Per-peer stream cap",
        name: "INV_WithinCap counts replies with requests: a reply past the cap is reset",
        axis: Tla,
        anchor: "crates/database/p2p/src/two_stream/runner.rs TwoStreamRunner::run; crates/database/p2p/src/two_stream/peer_streams.rs PeerStreams::admit",
        model_ref: "MC_PeerStreamCap_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The receive gate: a replication filter asked when a push arrives,
        // before its head is stored, and again on a fetched CAR, before it is
        // imported; the merge asks again as the backstop.
        family: "Receive gate",
        name: "INV_ArrivalRefusalStoresNothing / INV_FetchRefusalImportsNothing / INV_RefusedNeverMerged / INV_KeptIsKept",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/receive_gate.rs SyncCoordinator::gate_arrival, SyncCoordinator::gate_fetched; crates/database/db/src/receive_gate.rs DbReceiveGate::allows",
        model_ref: "MC_ReceiveGate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Composes the sender's markers, the receiver's admission and durable
        // records, the fetch pool and the hub's forward. The GREENs are the
        // target contract, not main: main violates this today, and REDs
        // F1, F2, F3, F4 and both halves of F5 are main's behaviour.
        // Assumes no kill between a local commit and its marker registration:
        // the drain before P2P stops covers a graceful stop only, and no
        // durable state covers that window.
        family: "P2P delivery liveness",
        name: "INV_ObligationOwned — every head merged on a sender stays owed over each edge until the receiver merged it or holds a durable record whose DAG brings it, judged by ancestry",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/head_scope.rs ScopeDecision::of; crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/database/p2p/src/sync/coordinator/broadcast.rs admit_replicator_push; crates/database/p2p/src/sync/replication/handlers.rs handle_block_received; crates/database/db/src/merge/txn_broadcaster.rs SyncTxnBroadcaster::submit_update; crates/entry-points/defra-node/src/lib.rs Node::shutdown; crates/surfaces/p2p-adapter/src/iroh_peer/peer.rs IrohPeer::assemble",
        model_ref: "MC_P2PDelivery_{Green_Receiver,Green_E2E,Red_F1_PriorityCover,Red_F2_MarkerAfterCommit,Red_F3_DischargeBeforeForward,Red_F4_LoopFirst,Red_F5_FanoutAbort,Red_F5_FailedDropped}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Fairness goes to the dispatch clock, the backoff timer, the resync
        // sweep, the recorder, the retry sweep and slotted fetch tasks, never
        // to a waiting root.
        // The GREENs are the target contract, not main: main violates this
        // today through F8 and F9, which are main's behaviour. The receiver
        // rules #2548 put on main are fixed in the model, and their REDs live
        // in MC_SyncOwnership_Dispatch and MC_SyncOwnership_Memory. The
        // receiver GREEN keeps one of its two global permits out of the pull
        // fetch's reach, which the code's limiter does not;
        // Finding_AdvisoryPermits is that gap. Eventual connectivity and the
        // bounded instance are assumed.
        family: "P2P delivery liveness",
        name: "LIVE_HeadDelivered — a deliverable head reaches every node its replicator edges lead to, through any hub, under task-level fairness only",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs dispatch_due_pending_dag_fetches, DagFetchLimiter; crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag, fetch_dag_attempt; crates/database/p2p/src/sync/coordinator/dag_retry.rs CLOCK_ATTEMPT_QUANTUM; crates/database/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::finish_clock_attempt; crates/database/p2p/src/sync/manager/process/pending_dag.rs PendingDagLease::finish_attempt, due_pending_dag_retries, resync_persisted_pending_dags, records_after_resync_cursor, consume_pending_merge_continuation; crates/database/p2p/src/sync/manager/pending.rs PendingDag::is_expired, PENDING_ROTATE_AFTER_DISPATCHES; crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass; crates/database/db/src/merge/push_docs.rs retry_doc",
        model_ref: "MC_P2PDelivery_{Green_Receiver,Green_E2E,Red_F8_SlotBeforePermit,Red_F9_MergeFifo,Finding_AdvisoryPermits}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Main violates this (F27): connect_with_cache does not adopt the
        // connection it dials, while the accepting side caches it for its own
        // requests, which then wait 30 s and retire it. The gents rc1 pin
        // (b50c667e9) already adopts every new connection.
        family: "P2P delivery liveness",
        name: "INV_ReverseBorrowServed — no node sends a request on a connection its peer does not serve",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_rpc.rs connect_with_cache, retire_timed_out_connection, REQUEST_RESPONSE_TIMEOUT; crates/database/p2p/src/iroh/endpoint_streams.rs handle_incoming",
        model_ref: "MC_P2PDelivery_{Green_E2E,Red_F27_HalfRegistered}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The binary built with `model-trace` logs one event per model
        // action; TLC replays a recorded run through the model under the
        // fixed design's knobs. Two recorded runs and eleven planted RED
        // configurations, one per replay guard, run in run-all.sh; the live
        // replay needs DEFRA_TRACE_BINARY and a JDK, and runs in the
        // Conformance job.
        family: "P2P delivery liveness",
        name: "TRACE_P2PDelivery — a recorded run of the binary is a behaviour of P2PDelivery: every logged event is a model step from the state the earlier ones left, the replayed states keep INV_ObligationOwned and INV_Bounded, and the run settles",
        axis: Tla,
        anchor: "crates/database/p2p/src/model_trace.rs Event; proofs/src/model_trace.rs Trace::abstracted, replay; proofs/tests/behavioral/model_trace.rs two_node_push_replays_through_p2p_delivery, hub_relay_replays_through_p2p_delivery",
        model_ref: "Trace_P2PDelivery.tla; MC_P2PDelivery_Trace_{TwoNode,HubRelay,Red_*}.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/model_trace.rs two_node_push_replays_through_p2p_delivery, hub_relay_replays_through_p2p_delivery",
    },
    Property {
        // The witnesses are integration tests outside this conformance
        // binary: p2p_admission_restart and issue1154_repro. Assumes a pending
        // store is installed: without one the registration, and so the ack, is
        // backed in memory only. Every production embedding installs one.
        family: "P2P delivery liveness",
        name: "INV_AckBacked — a success ack stays backed across a hub restart by a merge, a resident registration or a durable record",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/database/p2p/src/sync/manager/process/pending_dag.rs resync_persisted_pending_dags; tools/integration-test/tests/legacy/p2p_admission_restart.rs",
        model_ref: "MC_PendingDagRestart_{Green,Red_ProcessLocal}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The witness is the p2p_admission integration test, outside this
        // conformance binary.
        family: "P2P delivery liveness",
        name: "INV_SuccessImpliesRegisteredOrMerged — a PushLog at pending capacity is nacked, never acked with its registration dropped",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; tools/integration-test/tests/legacy/p2p_admission.rs fan_in_pushlog_admission_no_silent_divergence",
        model_ref: "MC_PushLogAdmission_{Green,Red_SuccessOnFull}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // gate_fetched judges only roots with a resident pending record. Pulls
        // and expired unacknowledged entries take the Walk path: their CAR is
        // imported unchecked and refused at merge.
        family: "Receive gate",
        name: "INV_FetchRefusalImportsNothing holds for pushed roots only",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/receive_gate.rs SyncCoordinator::gate_fetched",
        model_ref: "MC_ReceiveGate_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // One-to-many child paging (#3051): the scan stays unbounded and each
        // parent stops only when the selected index guarantees the child order.
        family: "Per-parent child paging",
        name: "INV_EveryParentHasPage / INV_EarlyStopOnlyWhenOrdered",
        axis: Tla,
        anchor: "crates/engine/query/src/planner/joins/child_plan.rs build_relation_child_plan; crates/engine/query/src/plan/type_join/type_join_many/children.rs TypeJoinMany::next_per_parent",
        model_ref: "MC_ChildLimitPaging_{Green,Red_UnorderedStop}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Per-parent child paging",
        name: "stop_after_parent_page",
        axis: Lean,
        anchor: "crates/engine/query/src/plan/type_join/type_join_many/children.rs TypeJoinMany::next_per_parent",
        model_ref: "ChildPage.lean",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Document save",
        name: "INV_NoFork / INV_DeletedStaysDeleted / INV_RefusedOnlyDeleted",
        axis: Tla,
        anchor: "crates/engine/query/src/documents/mod.rs save; crates/entry-points/defra-node/src/ops/document.rs Node::save_doc; tools/integration-test/tests/testo/mutation/update/with_delete.rs update_save_deleted_doc_does_nothing",
        model_ref: "MC_DocumentSave_{Green,Red_NoDeletedCheck,Red_SplitTxn}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Document save",
        name: "decide_refuses_deleted / refuse_only_deleted / save_never_forks / save_under_given_id",
        axis: Lean,
        anchor: "crates/engine/query/src/documents/mod.rs save; crates/engine/query/src/plan/mutation/upsert.rs UpsertNode::next",
        model_ref: "DocumentSave (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Mutation value checks",
        name: "Rfc3339Nano.no_trailing_zero / trim_restores / all_zero_trims_to_nothing / nonzero_keeps_a_digit",
        axis: Lean,
        anchor: "crates/building-blocks/document/src/encoding.rs trim_rfc3339_trailing_zeros, format_time_rfc3339_nano",
        model_ref: "Rfc3339Nano (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Mutation value checks",
        name: "DocIdText.length_refusal_only_for_unaccepted / accepted_length_never_says_length / digits_length / parse_accepts",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/doc_id.rs parse_uuid, ParsedDocId::from_string",
        model_ref: "DocIdText (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Mutation value checks",
        name: "SecondaryRelation.primary_never_refused / secondary_id_refused / same_answer_for_relation_and_id / checks_agree",
        axis: Lean,
        anchor: "crates/engine/query/src/runner/mutation_inputs.rs normalize_mutation_input_fields, input_field_name; crates/building-blocks/document/src/document.rs Document::set",
        model_ref: "SecondaryRelation (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Cursor tokens (#2481): a token is {d, k}, and an older token with an
        // empty `o` still decodes. Tokens are pinned byte for byte by
        // crates/foundation/cursor/tests/go_fixtures.rs; no conformance-binary test
        // reaches them. Boundary.
        family: "Cursor tokens",
        name: "encode_has_no_o / decode_encode / decode_legacy_o: a token is {d, k}",
        axis: Lean,
        anchor: "crates/foundation/cursor/src/lib.rs Cursor::encode, Cursor::decode",
        model_ref: "CursorToken (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A write's own enableSigning picks the flag its key is resolved
        // with, once, at the edge, before the write runs; no conformance
        // binary test reaches the choice of key. Boundary, bound by the unit
        // tests on each edge and the testo scenario.
        family: "Per-operation signing choice",
        name: "false_never_signs / true_signs_with_the_picked_key / true_signs_when_there_is_a_key / absent_follows_the_node / old_left_true_unsigned",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/request_context.rs RequestContext::signing; crates/foundation/defra-core/src/signing.rs resolve_signing_config_with_flag; crates/surfaces/http/src/query_context.rs request_context; crates/entry-points/ffi/src/document/run.rs DocumentCall::begin; crates/entry-points/defra-node/src/lib.rs execute_prepared_request",
        model_ref: "SigningOverride (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/foundation/defra-core/src/request_context.rs signing_override_false_hides_the_key_and_others_keep_it; crates/surfaces/http/src/query_context.rs enable_signing_overrides_the_node_either_way; crates/entry-points/ffi/src/document/signing_tests.rs opting_out_of_a_save_many_leaves_the_blocks_unsigned_on_a_signing_node; tools/integration-test/tests/testo/signature/per_op_signing.rs signature_with_per_op_signing_enabled_on_a_node_not_signing_signs_every_commit",
    },
    Property {
        // #2724: add_view accepts a transform CID only if it is non-empty and
        // held, stores it on the view's query source, and the definition's
        // JSON reads it back unchanged.
        family: "Add view with a lens transform",
        name: "ViewQuerySource.round_trip / definition_round_trip / stored_transform_is_accepted",
        axis: Lean,
        anchor: "crates/foundation/schema/src/source.rs QuerySource::with_transform; crates/foundation/schema/src/collection.rs CollectionVersion; crates/entry-points/defra-node/src/db_impls.rs DbSchemaOps::add_view; crates/database/db/src/collection/ops/create.rs DB::require_view_transforms",
        model_ref: "ViewQuerySource (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // #2724: the NAC, empty-CID and lens checks against the view's commit,
        // while the lens arrives (added locally or fetched by P2P sync) and
        // the permission changes. Both arrivals commit the lens config, and
        // the create transaction reads it (require_view_transforms);
        // NAC ViewAdd is still read before it, a known gap (#2736):
        // Green_NacBefore and Red_KnownGap_NacBefore.
        family: "Add view with a lens transform",
        name: "INV_NoEmptyTransform / INV_StoredTransformExists / INV_StoredIsAccepted / INV_NoViewWithoutPermission / INV_NoValidRejected",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/db_impls.rs DbSchemaOps::add_view; crates/database/db/src/collection/ops/create.rs DB::create_view_collections_as, DB::stage_new_collections, DB::require_view_transforms; crates/database/db/src/definition/migration/committed_lenses.rs DB::add_lens, DB::commit_synced_lens, write_lens_config; crates/surfaces/p2p-adapter/src/version_syncer.rs sync_lens; crates/surfaces/p2p-adapter/src/transport_version_syncer.rs sync_lens",
        model_ref: "MC_AddViewTransform_{Green,Green_NacBefore,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // #2965: float equality is exact, both in query filters and in index
        // matching; a tolerance can make `0` match `1e-300`.
        family: "Float filter equality",
        name: "exact_accepts_same / epsilon_accepts_distinct / tolerant_has_rejects",
        axis: Lean,
        anchor: "crates/engine/query/src/mapper/filter/eval/operators.rs values_equal; crates/engine/storage/src/index/matcher/mod.rs values_equal",
        model_ref: "FloatEquality (lake build); MC_FloatEquality_{Green,Red_Epsilon}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/query/src/mapper/filter/eval/operators.rs float_equality_is_exact; crates/engine/storage/src/index/matcher/tests.rs test_float_equality_is_exact; tools/integration-test/tests/testo/query/simple/with_filter/with_eq_float_zero.rs float_zero_equality_is_exact_on_indexed_and_unindexed_fields, float_zero_inequality_is_exact_on_indexed_and_unindexed_fields",
    },
    Property {
        // #2724: verify-signature's NAC, document ACP and block checks decide
        // at the caller's transaction view, or the read-only one the pool path
        // opens: NAC through check_node_access_in_txn, document ACP bound to
        // the view's ACP store.
        family: "Verify block signature",
        name: "INV_VerifiedAtOnePoint / INV_NoValidRejected",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/ops/block.rs NodeOps::verify_block_signature; crates/database/db/src/txn/registry/blocks.rs DbTransactionRegistry::verify_block_signature_in_txn_as; crates/database/db/src/block/verify.rs verify_block_signature_as, verify_block_signature_in_txn_as, verify_block_signature_with_txn, load_authorized_block_signature; crates/database/db/src/access/nac.rs DB::check_node_access_in_txn",
        model_ref: "MC_VerifySignature_{Green_*,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "One-to-many child paging",
        name: "LIVE_EveryParentPage / INV_RedSecondParentKeepsPage: a child page is filled per parent, not by one global scan bound",
        axis: Tla,
        anchor: "crates/engine/query/src/plan/type_join/type_join_many/children.rs TypeJoinMany::next_per_parent; crates/engine/query/src/plan/type_join/type_join_many/node.rs TypeJoinMany::with_limit, with_offset",
        model_ref: "MC_ChildPagePerParent_{Green,Red_GlobalBound}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/query/one_to_many/with_limit.rs query_one_to_many_with_child_limit_keeps_a_two_parent_page",
    },
    Property {
        // #2924: a host resolver configuration is read the way Go reads it
        // (rejected lines and words skipped, the rest kept); one left with no
        // nameservers, or that cannot be read, gets Go's local nameservers
        // instead of failing the node's start.
        family: "DNS resolver fallback",
        name: "DnsResolverFallback.chosen_has_nameservers / usable_read_passes_through / accepted_lines_survive / kept_lines_accepted / rejected_nameserver_dropped / failed_read_falls_back",
        axis: Lean,
        anchor: "crates/database/p2p/src/host/transport/resolver.rs read_file, parse_lenient, usable_line, accepted, resolver_config, fallback_config, FALLBACK_NAMESERVERS",
        model_ref: "DnsResolverFallback (lake build)",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/src/host/transport/resolver_tests.rs a_failed_read_falls_back_to_the_localhost_nameservers, a_read_with_nameservers_passes_through_unchanged, a_systemd_resolved_stub_passes_through, an_unknown_option_keeps_the_hosts_nameserver, a_bad_nameserver_line_keeps_the_good_ones, a_bad_nameserver_line_never_uses_its_comment, an_unknown_directive_keeps_the_hosts_nameserver, a_bad_search_domain_keeps_the_others, a_file_without_nameservers_keeps_its_search_and_options, a_comment_that_is_not_utf8_keeps_the_hosts_nameserver, a_missing_file_falls_back_to_the_localhost_nameservers, the_fallback_matches_the_lean_model",
    },
];
