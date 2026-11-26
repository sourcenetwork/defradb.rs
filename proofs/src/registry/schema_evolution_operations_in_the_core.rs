use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "client, db, lens, query, http, cli, ffi, defra-node";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Schema-evolution operations in the core",
        name: "INV_Keep / INV_Match / INV_Real: synced definitions preserve only the matching placeholder's transform and leave held definitions unchanged",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/definition.rs DbMergeHandler::process_collection_definition_delta",
        model_ref: "MC_SyncedMigrationTransform_{Green,Red_Drop,Red_Head}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/merge/collection_predecessor.rs",
    },
    Property {
        family: "Schema-evolution operations in the core",
        name: "SyncedMigrationTransform.matching_preserved / mismatched_not_carried / no_edge_no_transform / real_unchanged / absent_no_transform",
        axis: Lean,
        anchor: "crates/database/db/src/merge/merge_handler/definition.rs DbMergeHandler::process_collection_definition_delta",
        model_ref: "SyncedMigrationTransform (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/merge/collection_predecessor.rs",
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
        // A partially transformed document remains at its source version so
        // a lens registered later still runs.
        family: "Schema-evolution operations in the core",
        name: "INV_NoPartialStamp",
        axis: Tla,
        anchor: "crates/database/db/src/definition/lens.rs migration_path_transforms; crates/database/db/src/definition/migration/helpers.rs migrate_to_active; crates/database/db/src/definition/migration/reindex/page.rs ReindexPage::walk",
        model_ref: "MC_PartialLensPath_{Green,Red_Partial}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/issues/t_3044.rs",
    },
    Property {
        // The completeness guard holds only when every hop has a lens.
        family: "Schema-evolution operations in the core",
        name: "PartialLensPath.partial_not_materialized / empty_not_materialized / missing_hop_not_materialized / filled_hop_materialized",
        axis: Lean,
        anchor: "crates/database/db/src/definition/lens.rs migration_path_transforms; crates/database/db/src/definition/migration/reindex/page.rs ReindexPage::walk",
        model_ref: "PartialLensPath (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/issues/t_3044.rs",
    },
    Property {
        // Removing a transform must not invert native documents.
        family: "Schema-evolution operations in the core",
        name: "INV_NativePreserved",
        axis: Tla,
        anchor: "crates/database/db/src/definition/migration/reindex/page.rs ReindexPage::walk",
        model_ref: "MC_RemovedLensOrigin_{Green,Red_Inverse}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/collection_version/updates/remove/removed_transform_reindex.rs removing_transform_preserves_old_and_new_documents",
    },
    Property {
        // Native and materialized documents can have identical stored values.
        family: "Schema-evolution operations in the core",
        name: "RemovedLensOrigin.inverse_changes_native / preserving_keeps_native / same_stored_value_different_origins",
        axis: Lean,
        anchor: "crates/database/db/src/definition/migration/reindex/page.rs ReindexPage::walk",
        model_ref: "RemovedLensOrigin (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/collection_version/updates/remove/removed_transform_reindex.rs removing_transform_preserves_old_and_new_documents",
    },
    Property {
        // An explain names the direct lookup index, or the child scan index.
        family: "Schema-evolution operations in the core",
        name: "INV_NameMatches",
        axis: Tla,
        anchor: "crates/engine/query/src/plan/type_join/mod.rs scan_node_with_index_name; crates/engine/query/src/plan/type_join/type_join_one.rs explain_execute_inner",
        model_ref: "MC_ExplainIndexName_{Green,Green_Fallback,Red_Lookup}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/query/src/plan/type_join/mod.rs direct_lookup_index_takes_precedence_over_child_plan_index",
    },
    Property {
        // A lookup name takes precedence, with the child name as fallback.
        family: "Schema-evolution operations in the core",
        name: "ExplainIndexName.lookup_wins / child_is_fallback / no_index_no_name",
        axis: Lean,
        anchor: "crates/engine/query/src/plan/type_join/mod.rs scan_node_with_index_name",
        model_ref: "ExplainIndexName (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/query/src/plan/type_join/mod.rs direct_lookup_index_takes_precedence_over_child_plan_index",
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
        // #3052: the reindex a schema change owes fixes only the index and
        // leaves each document stored as written, so a lens added or removed
        // afterwards still applies to it as it stands (Red_WalkStamps).
        family: "Schema-evolution operations in the core",
        name: "INV_ReadsFollowWrites",
        axis: Tla,
        anchor: "crates/database/db/src/definition/migration/reindex/mod.rs DB::rebuild_settling; crates/database/db/src/definition/migration/reindex/page.rs ReindexPage::walk; crates/database/db/src/definition/patch/in_place.rs update_version_in_place",
        model_ref: "MC_LensIndexAnswer_{Green,Red_WalkStamps}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/collection_version/migrations/identity_settlement.rs identity_settlement_before_transform_should_apply_later_migration",
    },
    Property {
        // #3052: a materialization never resumes an owed reindex, a rebuild
        // that writes values back resumes only a record that does too, and a
        // resumed page walks the way its record says.
        family: "Schema-evolution operations in the core",
        name: "INV_RebuiltAll",
        axis: Tla,
        anchor: "crates/database/db/src/definition/migration/reindex/mod.rs DB::reindex_in_pages; crates/database/db/src/definition/migration/reindex/page.rs ReindexPage::step; crates/database/db/src/definition/migration/reindex/progress.rs read",
        model_ref: "MC_MaterializeResume_{Green,Red_Resume,Red_ResumeAnyMode,Red_SettleIgnoresMode}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/index/deferred.rs materializing_after_a_stopped_settlement_advances_every_document, reindexing_with_migrations_after_a_stopped_settlement_writes_back_every_document",
    },
    Property {
        // A walk from the start saves every document; one resumed from a
        // settlement's cursor misses those the settlement walked. The resume
        // decision reuses a record only for a rebuild it already serves.
        family: "Schema-evolution operations in the core",
        name: "MaterializeResume.start_over_saves_all / resume_misses / reuses_serves",
        axis: Lean,
        anchor: "crates/database/db/src/definition/migration/reindex/mod.rs DB::reindex_in_pages",
        model_ref: "MaterializeResume (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/index/deferred.rs materializing_after_a_stopped_settlement_advances_every_document, reindexing_with_migrations_after_a_stopped_settlement_writes_back_every_document",
    },
    Property {
        // A counter increment from a peer still at the source schema version
        // applies to that source projection, then the result migrates once.
        // A lens with no inverse cannot give that projection back, so the
        // increment is added to the migrated value as it is and still lands.
        // That agrees with a source-version receiver only for a lens that
        // shifts the counter or leaves it alone; a forward-only doubling lens
        // is the checked limit.
        // A whole active-document put is used for the translated result so a
        // linked non-operation field cannot make the merge skip it.
        family: "Schema-evolution operations in the core",
        name: "INV_TransformedOnce: an old-version counter increment is translated once",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/field_ops.rs counter_on_active_version, apply_field_op_once; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document",
        model_ref: "MC_MigratedCounterMerge_{Green,Green_ForwardOnly,Green_Scale,Green_ScaleForwardOnly,Red_Double,Red_Stuck,Red_ScaleForwardOnly}.cfg",
        tiers: &[Contract],
        bound_by: "tools/integration-test/tests/testo/issues/t_2916.rs an_old_version_counter_update_merges_through_a_lens_once, an_old_version_counter_update_merges_through_a_forward_only_lens, an_old_version_counter_update_adds_to_a_counter_a_forward_only_lens_wrote, an_old_version_counter_update_keeps_the_fields_the_lens_wrote, an_old_version_counter_update_lands_on_a_document_stored_at_the_source_version",
    },
    Property {
        // The Lean rule behind the TLA+ invariant: the source value plus the
        // peer's delta migrates once, while translating the already-migrated
        // value again can differ. Adding the delta to the migrated value
        // gives the same answer for a lens that shifts the counter or leaves
        // it alone, which is what a forward-only lens gets, and not for a
        // lens that doubles it.
        family: "Schema-evolution operations in the core",
        name: "MigratedCounterMerge.translated_once / double_apply_differs / direct_add_matches_shift / direct_add_matches_untouched / direct_add_misses_scale",
        axis: Lean,
        anchor: "crates/database/db/src/merge/merge_handler/field_ops.rs counter_on_active_version, apply_field_op_once; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document",
        model_ref: "MigratedCounterMerge (lake build)",
        tiers: &[Contract],
        bound_by: "tools/integration-test/tests/testo/issues/t_2916.rs an_old_version_counter_update_merges_through_a_lens_once, an_old_version_counter_update_merges_through_a_forward_only_lens, an_old_version_counter_update_adds_to_a_counter_a_forward_only_lens_wrote, an_old_version_counter_update_keeps_the_fields_the_lens_wrote, an_old_version_counter_update_lands_on_a_document_stored_at_the_source_version",
    },
];
