use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Encrypted index changes outside a transaction",
        name: "INV_NoLostIndex / INV_NoValidRefused",
        axis: Tla,
        anchor: "crates/database/db/src/collection/encrypted_index.rs DB::add_encrypted_index, DB::delete_encrypted_index, DB::rewrite_definition_in, definition_in; crates/database/db/src/txn/attempt.rs DB::transact_db; crates/entry-points/defra-node/src/ops/encrypted_index.rs NodeOps::add_encrypted_index, NodeOps::delete_encrypted_index; crates/entry-points/cli/src/encrypted_index_adapter.rs EncryptedIndexAdapter",
        model_ref: "MC_EncryptedIndexAutocommit_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
