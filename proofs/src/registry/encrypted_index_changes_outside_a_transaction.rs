use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, defra-node, cli";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Encrypted index changes outside a transaction",
        name: "INV_NoLostIndex / INV_NoValidRefused",
        axis: Tla,
        anchor: "crates/database/db/src/collection/encrypted_index.rs DB::add_encrypted_index, DB::delete_encrypted_index, DB::rewrite_definition_in, definition_in; crates/database/db/src/txn/attempt.rs DB::transact_db; crates/database/db/src/collection/encrypted_index/client/mod.rs DbTransactionRegistry::add_encrypted_index_as, DbTransactionRegistry::delete_encrypted_index_as",
        model_ref: "MC_EncryptedIndexAutocommit_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
