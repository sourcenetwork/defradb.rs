use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "chunk-split, schema, db, query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Text chunking",
        name: "INV_TextChunksExist / INV_NoChunkOfDeletedParent / INV_ValidWritesSucceed",
        axis: Tla,
        anchor: "crates/database/db/src/chunk/sync/mod.rs plan_chunks, ChunkWrites::create_missing, ChunkWrites::delete_all; crates/database/db/src/chunk/sync/lookup.rs ChunkCollection::existing_of; crates/database/db/src/write/autocommit/one_op.rs AutoCommitMutator::in_batch; crates/database/db/src/write/autocommit/batch.rs BatchMutator::update_as, delete_as; crates/database/db/src/write/doc.rs DbDocMutator",
        model_ref: "MC_ParentChunks_Green.cfg / MC_ParentChunks_Red_SeparateTxn.cfg / MC_ParentChunks_Red_NoCascade.cfg / MC_ParentChunks_Red_NoValidation.cfg / MC_ParentChunks_Red_NoLookup.cfg",
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
        name: "INV_SettledPeersAgree / INV_SettledPeersShowSameDocument / INV_NoForgedChunk / INV_ChunksNeverChange / INV_UsedChunksStay / INV_DeletesNotLost / INV_TextChunksStored / INV_OwedIsEncrypted / INV_EachChunkOnce / INV_PlainChunkStoredOnce",
        axis: Tla,
        anchor: "crates/engine/query/src/plan/chunk_parent.rs parent_hashes; crates/database/db/src/chunk/sync/mod.rs plan_chunks; crates/building-blocks/crdt/src/field/lww.rs merge; crates/database/db/src/chunk/guard.rs check_chunk_merge; crates/database/db/src/chunk/peer_delete.rs delete_must_wait; crates/database/db/src/merge/merge_handler/composite_persist.rs; crates/database/db/src/merge/merge_handler/doc_identity.rs resolve_composite_doc_id_in_txn; crates/database/db/src/merge/merge_handler/composite.rs process_composite_delta_locked; crates/database/db/src/chunk/rebuild.rs build_missing; crates/database/db/src/chunk/sync/write.rs ChunkWrites::create; crates/database/db/src/chunk/follow.rs current_chunk_ids; crates/engine/query/src/plan/chunk_once.rs shown_copy; crates/engine/query/src/plan/chunk_gate.rs ChunkGate::allows",
        model_ref: "MC_ChunkMerge_Green.cfg / MC_ChunkMerge_Green_Delete.cfg / MC_ChunkMerge_Green_Encrypted.cfg / MC_ChunkMerge_Red_NoHashCheck.cfg / MC_ChunkMerge_Red_MutableChunk.cfg / MC_ChunkMerge_Red_NoDeleteGuard.cfg / MC_ChunkMerge_Red_UsedDeleteOnly.cfg / MC_ChunkMerge_Red_RefuseDelete.cfg / MC_ChunkMerge_Red_UnstoredDelete.cfg / MC_ChunkMerge_Red_NoRebuild.cfg / MC_ChunkMerge_Red_SignedChunkIds.cfg / MC_ChunkMerge_Red_EncryptedNotShipped.cfg / MC_ChunkMerge_Red_EncryptedTwice.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "INV_ShownIsLiveAndReadable / INV_ShownIsTheText / INV_ShownPassesOwnApp / INV_ServedIsReadable / INV_DelegatedKeyInScope",
        axis: Tla,
        anchor: "crates/engine/query/src/plan/chunk_parent.rs parent_hashes, verified_hash; crates/engine/query/src/plan/chunk_gate.rs ChunkGate::allows, ChunkGate::decide; crates/engine/query/src/plan/permission_filter.rs permission_check; crates/database/db/src/chunk/access.rs history_parents, read_owners, parent_collection; crates/engine/query/src/runner/chunk_access.rs chunk_readable, ChunkReads; crates/engine/query/src/planner/builder/mod.rs maybe_wrap_with_acp_filter; crates/surfaces/p2p-adapter/src/read_gate.rs DbBlockReadGate::may_read, DbBlockClassifier::classify; crates/database/db/src/block/verify.rs load_authorized_block_signature; crates/surfaces/http/src/handlers/events.rs can_observe_document_event; crates/database/kms/src/defra_kms.rs DefraKms::local_release_decision; crates/database/kms/src/nac_dac_policy.rs Delegation::covers",
        model_ref: "MC_ChunkRead_Green.cfg / MC_ChunkRead_Red_NoDeletedCheck.cfg / MC_ChunkRead_Red_NoHashCheck.cfg / MC_ChunkRead_Red_SharedMemo.cfg / MC_ChunkRead_Red_NoMergedCheck.cfg / MC_ChunkRead_Red_NoGrantCheck.cfg / MC_ChunkRead_Red_NoCutCheck.cfg / MC_ChunkRead_Red_NoResolveCheck.cfg / MC_ChunkRead_Red_NoOwnAppCheck.cfg / MC_ChunkRead_Red_DelegationScope.cfg",
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
];
