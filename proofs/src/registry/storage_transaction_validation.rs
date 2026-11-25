use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Storage transaction validation",
        name: "Validated point reads reject crossing writes",
        axis: Tla,
        anchor: "crates/engine/storage/src/backends/regolith/transaction.rs; proofs/tests/behavioral/ssi.rs",
        model_ref: "regolith@e844baefe838f3af76403afd46c4b42f1a0b98a5 proofs/tla/DefraLevel.tla",
        tiers: &[Behavioral, Boundary],
        bound_by: "proofs/tests/behavioral/ssi.rs ssi_write_skew_prevented",
    },
];
