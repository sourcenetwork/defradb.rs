use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, defra-node, cli (#2321)";

pub(super) const PROPERTIES: &[Property] = &[Property {
    family: "Add view",
    name: "INV_NoViewUnlessAllChecksPass / INV_AllOrNone / INV_BuiltOnlyCreated / INV_SameAnswer",
    axis: Tla,
    anchor: "crates/database/db/src/view/client/mod.rs DbTransactionRegistry::add_view, view_source; crates/database/db/src/collection/ops/create.rs DB::create_view_collections_as; crates/database/db/src/txn/registry/views.rs DbTransactionRegistry::add_view_in_txn; crates/entry-points/defra-node/src/db_impls.rs DbSchemaOps::add_view; crates/entry-points/cli/src/view_adapter.rs ViewAdapter::add_view",
    model_ref: "MC_AddView_{Green,Red_*}.cfg",
    tiers: &[Boundary],
    bound_by: "",
}];
