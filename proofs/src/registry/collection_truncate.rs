use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Collection truncate",
        name: "INV_NoRemovalWithoutPermission / INV_NoRemovalFromChunks / INV_FilteredNeverOnBranchable / INV_FilteredRemovesOnlyCommittedMatches / INV_FilteredRemovesOnlyCurrentMatches / INV_RetryExhaustionKeepsProgress / INV_MetadataExhaustionRefused / INV_BadFilterRemovesNothing / INV_UnfilteredRemovesAll / INV_UnfilteredEmptiesView / INV_UnfilteredClearsSearchRecords",
        axis: Tla,
        anchor: "crates/database/db/src/collection/ops/truncate/mod.rs DB::truncate_collection_as, DB::truncate_parsed_as, DB::truncate_target, conditions, DB::truncate; crates/database/db/src/collection/ops/truncate/all.rs DB::truncate_all, DB::truncate_documents, DB::truncate_collection_metadata, DB::truncate_metadata_in; crates/database/db/src/collection/ops/truncate/deferred.rs DB::truncate_deferred_indexes; crates/database/db/src/collection/ops/truncate/filtered.rs DB::truncate_matching, matching_doc_ids_in_attempt; crates/database/db/src/chunk/user_writes.rs ensure_admin_changeable; crates/database/db/src/merge/se/serve.rs store_artifacts, build_reply",
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
];
