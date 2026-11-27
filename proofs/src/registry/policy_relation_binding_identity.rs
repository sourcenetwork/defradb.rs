use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "schema, defra-core";

pub(super) const PROPERTIES: &[Property] = &[Property {
    family: "Policy relation binding identity",
    name: "unbound_is_legacy / ungoverned_is_legacy / governed_commits_to_binding / ungated_moves_an_ungoverned_identity",
    axis: Lean,
    anchor: "crates/foundation/schema/src/cid.rs field_to_delta_with_priority; crates/foundation/defra-core/src/block_delta.rs FieldDefinitionDeltaPayload",
    model_ref: "PolicyBindingIdentity (lake build)",
    tiers: &[Boundary],
    bound_by: "",
}];
