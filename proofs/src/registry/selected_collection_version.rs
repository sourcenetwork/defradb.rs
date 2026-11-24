use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, query, ffi";

pub(super) const PROPERTIES: &[Property] = &[
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
];
