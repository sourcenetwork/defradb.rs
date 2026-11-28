use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, defra-node, http, ffi, cli (#2321)";

pub(super) const PROPERTIES: &[Property] = &[Property {
    // #2321: every way in exports through the core, which checks the
    // `read-document` node permission before it reads a document.
    family: "Backup export access",
    name: "INV_NoReadWithoutPermission / INV_SameAnswer: an export reads only for a caller with read-document, the same on every edge",
    axis: Tla,
    anchor: "crates/database/db/src/backup/export.rs export_database_as; crates/entry-points/defra-node/src/ops/backup.rs NodeOps::export_backup; crates/entry-points/cli/src/backup_adapter.rs BackupAdapter; crates/entry-points/ffi/src/backup/export.rs defra_backup_export; crates/surfaces/http/src/handlers/backup.rs export",
    model_ref: "MC_BackupExportAccess_{Green,Red_NoCoreCheck,Red_FfiCopy}.cfg",
    tiers: &[Boundary],
    bound_by: "crates/entry-points/ffi/src/backup/tests.rs node_access_control_refuses_an_export_to_a_stranger; tools/integration-test/tests/testo/backup/node_access.rs an_export_by_the_node_owner_returns_the_documents, an_export_by_another_identity_is_not_authorized, an_export_with_no_identity_is_not_authorized",
}];
