use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #2991: a data directory holding Go DefraDB's Badger datastore is
        // refused with what to do before regolith opens it, so nothing there
        // changes; an unreadable MANIFEST fails the look and changes nothing
        // either; a regolith store or an empty directory is never refused.
        family: "Foreign store refusal",
        name: "ForeignStoreRefusal.badger_manifest_refused / key_registry_refused / unreadable_manifest_fails / regolith_store_opens / empty_directory_opens / refused_start_changes_nothing / read_failed_start_changes_nothing / only_an_unrefused_start_opens",
        axis: Lean,
        anchor: "crates/engine/storage/src/backends/regolith/foreign.rs refuse_badger, manifest_is_badger, BADGER_MANIFEST_MAGIC, BADGER_KEY_REGISTRY",
        model_ref: "ForeignStoreRefusal (lake build)",
        tiers: &[Contract],
        bound_by: "crates/engine/storage/src/backends/regolith/foreign.rs a_badger_manifest_is_refused_and_nothing_is_written, opening_a_badger_folder_fails_before_regolith_writes_there, a_badger_key_registry_alone_is_refused, an_unreadable_manifest_fails_with_its_read_error_and_writes_nothing, a_missing_empty_or_regolith_folder_passes, the_magic_matches_the_lean_model; crates/entry-points/cli/tests/go_datastore.rs the_binary_refuses_a_go_datastore_before_start_and_storage_stats",
    },
];
