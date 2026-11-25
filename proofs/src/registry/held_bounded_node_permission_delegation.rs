use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "acp, db, defra-node";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Held-bounded node permission delegation",
        name: "delegated_no_escalation / delegated_never_touches_admin / admin_behavior_preserved / admin_acceptances_kept / delegate_admin_refused / delegate_unheld_refused / delegate_admin_class_refused / delegate_wildcard_refused / delegate_privileged_delete_refused / delegation_not_transferable / admits_observe",
        axis: Lean,
        anchor: "crates/engine/acp/src/nac/delegation.rs DelegationFacts::admits, ADMIN_CLASS; crates/engine/acp/src/nac/node_acp/operations.rs NodeACP::authorize_change; crates/engine/acp/src/nac/node_acp/source.rs NodeACP::change_source_permission",
        model_ref: "NodeAcpDelegation (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/delegated_nac.rs delegation_matches_executed_lean_model, delegate_grants_and_revokes_only_held_non_admin_permissions, delegate_source_edges_follow_the_same_rule",
    },
];
