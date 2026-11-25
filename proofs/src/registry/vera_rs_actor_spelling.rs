use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "vera.rs actor spelling",
        name: "defra_idempotent / spellings_meet / other_unchanged / defra_same_actor / one_spelling_per_actor / wire_is_taken / wire_same_actor / defra_of_wire / wire_secp256k1_is_defra",
        axis: Lean,
        anchor: "crates/engine/vera-delegation/src/did_spelling.rs defra, wire, same_actor",
        model_ref: "VeraActorSpelling (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
