use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-index, db-merge";

pub(super) const PROPERTIES: &[Property] = &[
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
        family: "Index-maintenance consistency",
        name: "INV_FreshAttempt / INV_OpenCompatible / INV_FailureKeepsStamp / INV_FutureRefusedUntouched / INV_LegacyIntact — rebuild older indexes before stamping; refuse future formats",
        axis: Tla,
        anchor: "crates/database/db/src/definition/migration/format.rs CURRENT_INDEX_FORMAT, DB::stored_index_format, DB::migrate_index_format; crates/database/db/src/database/mod.rs DB::finish_open; crates/database/db/src/index/deferred/drain.rs; crates/database/db/src/index/manager/deferred.rs",
        model_ref: "MC_IndexFormatGuard_{Green,Red_SkipRebuild,Red_EarlyStamp,Red_DeleteLegacy,Red_UnguardedRecord,Red_StaleProgress,Red_Future}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/definition/format.rs",
    },
    Property {
        family: "Index-maintenance consistency",
        name: "IndexKeyspace.short_id_bytes_prefix_eq / tag_not_prefix_of_legacy / key_not_in_document_family / key_not_in_version_family — for modeled one-byte IDs and the first two-byte boundary, tagged index keys do not enter the document or version families; IndexKey.bytes is an index prefix, not a complete entry",
        axis: Lean,
        anchor: "crates/engine/storage/src/keys/datastore/index_key.rs INDEX_KEYSPACE_PREFIX, IndexDataStoreKey::index_prefix, IndexDataStoreKey::try_bytes",
        model_ref: "IndexKeyspace.short_id_bytes_prefix_eq / tag_not_prefix_of_legacy / key_not_in_document_family / key_not_in_version_family",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/definition/keyspace.rs index_keyspace_scans_stay_per_collection_across_short_ids; crates/engine/storage/src/keys/datastore/index_key.rs index_keyspace_is_tagged_across_the_byte_boundary, index_integer_encodings_are_prefix_free",
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
        name: "decodeRecord_encodeValues — a position record of field values reads back as exactly its entry keys",
        axis: Lean,
        anchor: "crates/database/db/src/index/deferred/record.rs encode_record, decode_record",
        model_ref: "PositionRecord (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Stores written before records kept field values hold whole keys;
        // such a record is never taken for one of values and still reads back.
        family: "Index-maintenance consistency",
        name: "decodeRecord_encode — a position record of whole keys still reads back as exactly its entry keys",
        axis: Lean,
        anchor: "crates/database/db/src/index/deferred/record.rs decode_record, decode_keys",
        model_ref: "PositionRecord (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
