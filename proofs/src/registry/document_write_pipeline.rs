use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, db-block-builder";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Document write pipeline",
        name: "INV_NoStoreBeforeComposite / INV_NoMintInAttempt / INV_OneKeyPerField / INV_CommittedWhole",
        axis: Tla,
        anchor: "crates/database/db/src/write/create.rs embed, identity, seed_identity, memo_identity, prepare_keys, compute, compute_many, persist; crates/database/db/src/write/autocommit/create.rs create_many_with_resolved_inputs, create_impl, run_create_attempt; crates/database/db/src/write/autocommit/batch.rs BatchMutator::create_as (with a memo); crates/database/db/src/write/doc.rs DbDocMutator::create_as, create_identity; crates/database/db/src/write/update.rs write_local, write_blocks; crates/database/db/src/write/autocommit/update.rs run_update_attempt; crates/database/db/src/write/autocommit/batch.rs BatchMutator::update_as (with a memo); crates/database/db/src/write/doc.rs DbDocMutator::update_as; crates/database/db/src/block/builder/write.rs build_document_blocks, encrypt_with_new_key",
        model_ref: "MC_WritePipeline_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
