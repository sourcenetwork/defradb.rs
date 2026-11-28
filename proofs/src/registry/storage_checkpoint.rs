use super::{Axis::*, Property, Tier::*};

pub(super) const CRATES: &str = "storage, cli";

const TEST: &str = "tools/integration-test/tests/testo/node/storage_checkpoint.rs checkpoint_preserves_configuration_and_signed_history";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Storage checkpoint",
        name: "ExistingUntouched / SuccessIsComplete",
        axis: Tla,
        anchor: "crates/entry-points/cli/src/commands/storage.rs StorageArgs::checkpoint; crates/engine/storage/src/backends/regolith/store.rs RegolithStore::checkpoint_to_new_directory",
        model_ref: "MC_StorageCheckpoint_{Green,Red_Existing,Red_Early}.cfg",
        tiers: &[Boundary],
        bound_by: TEST,
    },
    Property {
        family: "Storage checkpoint",
        name: "StorageCheckpoint.checkpoint_reads / StorageCheckpoint.checkpoint_genesis_id: consequences of Regolith's assumed copy contract",
        axis: Lean,
        anchor: "Regolith checkpoint copy contract (assumed); crates/engine/storage/src/backends/regolith/store.rs RegolithStore::checkpoint_to_new_directory (delegation)",
        model_ref: "StorageCheckpoint (lake build)",
        tiers: &[Boundary],
        bound_by: TEST,
    },
];
