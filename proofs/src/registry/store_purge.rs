use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "http, defra-node, cli, storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A purge is answered at once; the teardown runs outside the
        // request, the wipe only after a clean close, the node opens only
        // after the wipe, and the keyring is never touched.
        family: "Store purge",
        name: "INV_NoWipeWhileOpen / INV_NoOpenBeforeWipe / INV_NothingSurvives / INV_KeyringKept / INV_RefusedPurgeTouchesNothing / INV_PurgeOnlyInDevelopment / INV_OnePurgePerRun / INV_NoRestartAfterFailedTeardown / INV_SignerSurvivesRestart / LIVE_PurgeServesAgain: the store is wiped only once closed and opened only once wiped, nothing of it or the embedded Vera state survives, the keyring stays, a refused purge changes nothing, a purge is accepted only in development mode, one purge runs per start, a failed teardown wipes nothing and restarts nothing, nothing is wiped while another handle still holds the node or after a shutdown another caller ran, the Orbis signer defradb start registered survives the restart, a purged node serves again",
        axis: Tla,
        anchor: "crates/surfaces/http/src/handlers/utility.rs purge; crates/entry-points/defra-node/src/purge.rs PurgeGate, Node::purge_as, Node::shutdown_and_wipe; crates/engine/storage/src/wipe.rs wipe; crates/entry-points/cli/src/commands/start/run.rs Node::run_until; crates/entry-points/cli/src/commands/start/purge.rs outcome; crates/entry-points/cli/src/commands/start/mod.rs StartArgs::execute, StartArgs::hold_remote_signer",
        model_ref: "MC_StorePurge_{Green,Green_Vera,Green_MemoryVera,Green_Cli,Red_CliWipeAfterCloseFailed,Red_CliWipeAfterVeraStopFailed,Red_DoublePurge,Red_MemorySkipsVeraDirs,Red_NoHandlerStillStops,Red_OpenBeforeWipe,Red_PurgeOutsideDevelopment,Red_RestartAfterFailedWipe,Red_ShutdownInsideRequest,Red_WipeAfterOtherShutdown,Red_WipeBeforeClose,Red_WipeKeyring,Red_WipeWhileHandleHeld,Red_WipeWhileVeraHeld,Red_SignerDropped}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/node/purge.rs purge_removes_every_collection_and_document, purge_restarts_defradb_start_on_the_same_address, purge_keeps_a_keyring_held_identity_and_peer_key, purge_is_refused_without_development_mode; crates/entry-points/defra-node/tests/purge.rs a_node_another_handle_holds_is_shut_down_but_not_wiped, a_node_another_caller_shut_down_is_not_wiped; crates/entry-points/cli/src/commands/start/purge.rs a_failed_teardown_ends_the_purge_with_its_error_and_nothing_to_wipe; crates/entry-points/cli/src/commands/start/mod.rs the_orbis_signer_outlives_the_identity_clear_between_runs",
    },
    Property {
        // The removal set a purge builds: the data directory is dropped key
        // by key, never removed; only owned directories are removed, and
        // none of them holds the keyring or the data directory, with `.` and
        // `..` resolved before the comparison.
        family: "Store purge",
        name: "StorePurge.removal_is_owned_dirs / accepted_spares / data_dir_is_dropped_never_removed / removal_never_reaches_kept / memory_drops_nothing / cli_layout_is_accepted / climbing_keyring_is_refused / raw_check_misses_the_climb",
        axis: Lean,
        anchor: "crates/engine/storage/src/wipe.rs WipeTarget, wipe, check, resolve, resolve_removal, lexical; crates/entry-points/cli/src/commands/start/purge.rs wipe_target",
        model_ref: "StorePurge (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/storage/tests/wipe.rs wipe_empties_the_store_and_removes_only_owned_dirs, wipe_refuses_an_owned_dir_holding_a_kept_path, wipe_refuses_a_kept_path_that_climbs_back_into_an_owned_dir, wipe_refuses_a_data_dir_that_climbs_back_into_an_owned_dir, wipe_accepts_disjoint_paths_written_with_dots, wipe_refuses_a_kept_path_that_climbs_through_a_symlink_into_an_owned_dir, wipe_removes_an_owned_symlink_and_not_what_it_points_to",
    },
];
