use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Storage transaction validation",
        name: "Validated point reads reject crossing writes",
        axis: Tla,
        anchor: "crates/engine/storage/src/backends/regolith/transaction.rs; proofs/tests/behavioral/ssi.rs",
        model_ref: "regolith@055589006356738a31ee3b0e965d8e2a9c5bc057 proofs/tla/RepeatableRead.tla (MC_DefraLevel_* configurations; regolith has no DefraLevel.tla)",
        tiers: &[Behavioral, Boundary],
        bound_by: "proofs/tests/behavioral/ssi.rs ssi_write_skew_prevented",
    },
];
