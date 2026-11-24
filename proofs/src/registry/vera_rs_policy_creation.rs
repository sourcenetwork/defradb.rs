use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "vera.rs policy creation",
        name: "INV_OneRequestOnePolicy / INV_ReturnedIsOwn / INV_NoRefusalOfOwn / INV_NoForeignBlock / INV_SuppliedIdIsCallers",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/provider/create_policy.rs VeraRsProvider::create_policy_with, VeraRsProvider::adopt_existing; crates/engine/vera/src/vera_rs/provider/submission.rs VeraRsProvider::confirm_tx; crates/engine/vera/src/cosmos/dac.rs VeraDocumentACP::add_policy",
        model_ref: "MC_VeraPolicyCreate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
