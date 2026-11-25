use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "kms, p2p, db-merge, crdt";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Encryption reuses PriorityReconcile.lwwCM unchanged: ciphertext and
        // key timing affect when a value can materialize, not which value wins.
        // The behavioral legs preserve encrypted fields selected for filtered
        // replication and receive both encrypted siblings after a real restart,
        // proving identical DAGs and the decrypted winner.
        family: "Encrypted LWW restart/replay",
        name: "INV_NoFilteredLoss / INV_LwwWinner — encrypted delivery remains complete and LWW-convergent",
        axis: Tla,
        anchor: "crates/database/db/src/merge/push_docs.rs; crates/database/p2p/src/sync/coordinator/selective_car_access.rs; crates/building-blocks/crdt/src/field/lww.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/partition.rs convergence_encrypted_lww_restart_merge",
    },
    Property {
        // The model and focused KMS/merge unit tests cover the retry mechanics,
        // but the external harness cannot deterministically hold a DEK request
        // unavailable between ciphertext acknowledgement and receiver restart.
        family: "Encrypted LWW restart/replay",
        name: "INV_AckBacked — acknowledged encrypted replay remains re-drivable across transient KMS failure and restart",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/composite_fields.rs; crates/database/p2p/src/sync/pending_store/mod.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
