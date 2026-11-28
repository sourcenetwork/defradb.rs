use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, defra-node, http, cli";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Index changes in a transaction",
        name: "INV_WorkOnlyAfterCommit / INV_IsolatedUntilCommit / INV_SeesOwnChange / INV_BuiltWhenCommitReturns / INV_WorkSeesTxnWrites / INV_FailedChangeLeavesNothing",
        axis: Tla,
        anchor: "crates/database/db/src/index/in_txn.rs DB::create_index_in, DB::drop_index_in; crates/database/db/src/txn/registry/indexes.rs DbTransactionRegistry::create_index_in_txn, DbTransactionRegistry::drop_index_in_txn, DbTransactionRegistry::indexes_in_txn; crates/database/db/src/txn/registry/mod.rs finish_statement; crates/entry-points/defra-node/src/node_core/index.rs NodeCore::create_index, NodeCore::delete_index, NodeCore::list_indexes; crates/surfaces/http/src/handlers/index/go_api.rs go_create_index, go_list_indexes, go_delete_index; crates/database/db/src/collection/encrypted_index.rs DB::add_encrypted_index_in, DB::delete_encrypted_index_in, DB::encrypted_indexes_in, DB::all_encrypted_indexes_in; crates/database/db/src/txn/registry/indexes.rs DbTransactionRegistry::add_encrypted_index_in_txn, DbTransactionRegistry::delete_encrypted_index_in_txn, DbTransactionRegistry::encrypted_indexes_in_txn, DbTransactionRegistry::all_encrypted_indexes_in_txn; crates/database/db/src/collection/encrypted_index/client/mod.rs DbTransactionRegistry::add_encrypted_index_as, DbTransactionRegistry::delete_encrypted_index_as, DbTransactionRegistry::encrypted_indexes_as, DbTransactionRegistry::all_encrypted_indexes_as; crates/database/db/src/collection/ops/create.rs DB::stage_new_collections; crates/database/db/src/txn/registry/views.rs DbTransactionRegistry::add_view_in_txn, DbTransactionRegistry::refresh_views_in_txn",
        model_ref: "MC_IndexInTxn_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
