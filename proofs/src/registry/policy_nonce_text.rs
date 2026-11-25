use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Policy nonce text",
        name: "decode_encode / encode_length / encode_digits / parse_encode / encode_injective / decode_length / parse_length",
        axis: Lean,
        anchor: "crates/engine/acp/src/policy_nonce.rs PolicyNonce",
        model_ref: "VeraPolicyNonce (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
