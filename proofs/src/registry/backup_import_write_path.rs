use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, defra-node, http, ffi, cli";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #2716: every create and relation patch of an import is a request
        // on the request path of the edge it came in by, so it is signed,
        // broadcast and owned as that edge's creates are; a Node import is
        // one admitted request, run as a network task of its own that its
        // writes continue, so shutdown drains it and a caller that stops
        // waiting does not cut it short;
        // a batch's file IDs are checked just before it is saved, a refused
        // batch is saved again one document at a time, and the IDs are
        // registered by validated reads in one transaction, one document at
        // a time when another writer took one meanwhile.
        family: "Backup import write path",
        name: "INV_ImportSigned / INV_ImportReplicated / INV_HostFromEdge / INV_DriverOnCaller / INV_NoDuplicateAlias / INV_AliasNeverStolen / INV_FfiTxnRefused / INV_CreatorAsCreate / INV_SavedRegistered / INV_DrainWaitsForImport / INV_KeepsPrefix: an import writes as its edge writes a create",
        axis: Tla,
        anchor: "crates/database/db/src/backup/import.rs import_database_as, save, create, record; crates/database/db/src/backup/host.rs ImportHost, RunnerHost; crates/database/db/src/backup/batch.rs Batch::must_save_before; crates/database/db/src/backup/alias.rs register, check_free; crates/entry-points/defra-node/src/node_core/backup.rs NodeCore::import_backup, NodeCore::import_admitted; crates/entry-points/defra-node/src/ops/backup.rs NodeOps::import_backup_with, NodeHost; crates/entry-points/defra-node/src/lib.rs Node::as_request, Admit; crates/entry-points/defra-node/src/request_path.rs RequestPath::execute, RequestPath::admitted; crates/foundation/edge/src/handle.rs Edge::spawn_continuation; crates/surfaces/http/src/request_path.rs Server; crates/surfaces/http/src/pool_layer.rs runs_on_pool, ON_EDGE; crates/surfaces/http/src/handlers/backup.rs import; crates/entry-points/cli/src/commands/start/server_query.rs setup_query_runner; crates/entry-points/ffi/src/backup/import.rs defra_backup_import, FfiHost",
        model_ref: "MC_BackupImportWritePath_{Green,Red_OwnMutator,Red_NoSigningChoice,Red_AliasOutsideTxn,Red_AliasCheckAfterCreate,Red_BatchSkipsPendingAlias,Red_FfiViaNode,Red_AdapterOwnHost,Red_CallerIgnored,Red_ActorNoOne,Red_WritesAdmittedAlone,Red_ImportOnCallerTask,Red_RefusedBatchDropped,Red_RegistrationAllOrNothing,Red_PrepareOnPool,Red_FfiTxnAccepted,Red_ContentCollisionDropped,Red_ImportRouteOnPool}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/ops/backup_tests.rs shutdown_drains_a_running_import_with_its_writes_and_aliases, an_import_after_shutdown_begins_is_refused_before_it_writes, the_driver_runs_off_the_workers_and_saves_a_collection_in_one_write, a_saved_batch_gets_its_aliases_when_the_caller_stops_waiting; crates/entry-points/cli/src/backup_adapter_tests.rs an_id_taken_after_its_check_leaves_every_other_saved_document_its_id; crates/entry-points/ffi/src/backup/import_tests.rs an_import_on_a_signing_node_signs_every_block, node_access_control_refuses_an_import_to_a_stranger; crates/surfaces/http/src/handlers/backup_import_tests.rs the_import_driver_runs_off_the_workers_and_its_writes_on_them; tools/integration-test/tests/testo/backup/signed_import.rs an_import_on_a_signing_node_signs_every_block, an_import_reaches_a_replicator_set_before_it; tools/integration-test/tests/testo/backup/failed_import.rs a_failed_import_keeps_every_document_before_the_one_refused, an_import_refusing_a_value_keeps_every_document_before_it",
    },
    Property {
        // #2716: an import's request is the create request of its edge, so
        // it signs and is owned as a create; a batch with no file ID twice,
        // checked as a whole and saved again one document at a time when
        // refused, saves exactly what one document at a time saves: never a
        // file ID twice, and every document before the one refused.
        family: "Backup import write path",
        name: "BackupImportSigner.import_request_is_create_request / signing_node_signs_every_import / creator_law / cli_start_actor_falls_back_to_its_identity / saveBatch_eq_saveEach / no_duplicate_alias / keeps_prefix_until_refused / dropped_batch_loses_the_prefix / no_flush_saves_a_duplicate",
        axis: Lean,
        anchor: "crates/database/db/src/backup/import.rs import_database_as, save, create; crates/database/db/src/backup/batch.rs Batch::must_save_before; crates/entry-points/defra-node/src/lib.rs execute_prepared_request",
        model_ref: "BackupImportSigner (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/backup/import_creator.rs an_anonymous_import_is_owned_as_an_anonymous_create; tools/integration-test/tests/testo/backup/duplicate_alias.rs a_repeated_doc_id_fails_and_keeps_the_first_document; tools/integration-test/tests/testo/backup/failed_import.rs a_failed_import_keeps_every_document_before_the_one_refused",
    },
];
