use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "defra-core, http, ffi, cli";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A write's own enableSigning picks the flag its key is resolved
        // with, once, at the edge, before the write runs; no conformance
        // binary test reaches the choice of key. Boundary, bound by the unit
        // tests on each edge and the testo scenario.
        family: "Per-operation signing choice",
        name: "false_never_signs / true_signs_with_the_picked_key / true_signs_when_there_is_a_key / absent_follows_the_node / old_left_true_unsigned",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/request_context.rs RequestContext::signing; crates/foundation/defra-core/src/signing.rs resolve_signing_config_with_flag; crates/surfaces/http/src/query_context.rs request_context; crates/entry-points/ffi/src/document/run.rs DocumentCall::begin; crates/entry-points/defra-node/src/lib.rs execute_prepared_request",
        model_ref: "SigningOverride (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/foundation/defra-core/src/request_context.rs signing_override_false_hides_the_key_and_others_keep_it; crates/surfaces/http/src/query_context.rs enable_signing_overrides_the_node_either_way; crates/entry-points/ffi/src/document/signing_tests.rs opting_out_of_a_save_many_leaves_the_blocks_unsigned_on_a_signing_node; tools/integration-test/tests/testo/signature/per_op_signing.rs signature_with_per_op_signing_enabled_on_a_node_not_signing_signs_every_commit",
    },
];
