use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "gents";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Per-field LWW is the DefraDB data model: fields are not atomic across
        // a record, so a converged claim can mix different claimants' fields.
        // Consumers fence on one field (gents: execution_generation).
        family: "Multi-instance claim",
        name: "INV_EventualUnique per field — each AgentRequest field converges to one LWW winner (priority, then bytes)",
        axis: Tla,
        anchor: "crates/building-blocks/crdt/src/field/lww.rs LwwField::merge; gents:crates/gents/src/request/lifecycle/claim.rs claim_inner (foreign substrate, not the defradb.rs binary)",
        model_ref: "MC_Claim_Filtered_Eventual.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
