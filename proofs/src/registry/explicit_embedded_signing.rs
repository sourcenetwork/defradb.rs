use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "defra-node, query, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Explicit embedded signing",
        name: "admitted_binds",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/embedded_signer.rs EmbeddedSigner::from_config, admitted_signer; crates/entry-points/defra-node/src/request_path.rs RequestPath::execute",
        model_ref: "EmbeddedSigner (lake build); embedded_signer::native_admission_matches_executed_lean_model",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/embedded_signer.rs native_admission_matches_executed_lean_model",
    },
    Property {
        family: "Explicit embedded signing",
        name: "unsupported_refused",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/embedded_signer.rs EmbeddedSigner::from_config",
        model_ref: "EmbeddedSigner (lake build); embedded_signer::native_admission_matches_executed_lean_model",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/embedded_signer.rs invalid_or_missing_local_key_is_rejected, native_admission_matches_executed_lean_model",
    },
    Property {
        family: "Explicit embedded signing",
        name: "caller_signs_only_as_actor / no_key_signs_with_node / refused_only_a_bad_key: a caller's key signs only as the actor it belongs to, a call without a key is signed by the node, and only a call whose key is not admitted is refused",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/embedded_signer.rs admitted_signer; crates/entry-points/defra-node/src/ops/request.rs Node::admit, Node::admitted_caller; crates/entry-points/defra-node/src/request_path.rs RequestPath::execute; crates/entry-points/defra-node/src/lib.rs Node::execute_as",
        model_ref: "EmbeddedSigner (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/caller_key_admission.rs execute_as_refuses_a_key_of_another_actor, execute_as_does_not_sign_with_a_key_registered_for_the_request_did, execute_as_signs_with_the_callers_own_key",
    },
    Property {
        family: "Explicit embedded signing",
        name: "INV_NoForgery / INV_PresentedOnly / INV_Verifiable / INV_UnderDac: every entry point that writes goes through the one admission check, so no block is signed by another actor's key, by a key the caller did not hand over, by an unverifiable key, or under DAC bypass",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/embedded_signer.rs EmbeddedSigner::from_config, admitted_signer; crates/entry-points/defra-node/src/ops/request.rs Node::admit, Node::admitted_caller; crates/entry-points/defra-node/src/request_path.rs RequestPath::execute; crates/entry-points/defra-node/src/lib.rs Node::execute_as; crates/entry-points/defra-node/src/ops/backup.rs Node::import_backup_as",
        model_ref: "MC_EmbeddedSigner_{Green,Red_*}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/caller_key_admission.rs every_writing_entry_refuses_a_key_of_another_actor, import_backup_as_refuses_a_key_of_another_actor, import_backup_as_refuses_a_key_with_dac_bypass, execute_as_refuses_a_key_with_dac_bypass, execute_as_refuses_an_unsupported_key; crates/entry-points/defra-node/tests/embedded_signer.rs selected_actor_signs_create_update_and_respects_transaction_finality, selected_signer_does_not_bypass_document_acp",
    },
];
