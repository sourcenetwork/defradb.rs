use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // `execute_as` signs a node's writes through the shared signed-query
        // context while keeping its caller's identity and DAC bypass; a
        // missing signer fails closed. Boundary, bound by the focused node
        // signing test.
        family: "Node identity and signing",
        name: "NodeSigningChoice.execute_as_keeps_the_caller / execute_as_signs_as_the_caller / execute_as_unsigned_without_signing / execute_as_refuses_a_missing_signer",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::execute_as; crates/entry-points/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/foundation/defra-core/src/request_context.rs RequestContext::with_signing_session",
        model_ref: "NodeSigningChoice (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/execute_as_signing.rs execute_as_signs_writes_and_keeps_the_caller_identity",
    },
    Property {
        // A node's identity and its signing are separate choices, as in Go:
        // an identity-only node runs each request as its caller and never
        // signs, and a node built without `with_identity_only` runs requests
        // exactly as before.
        family: "Node identity and signing",
        name: "NodeSigningChoice.default_build_unchanged / identity_only_runs_as_caller / identity_only_never_signs / identity_only_never_refused",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::execute_prepared_request; crates/entry-points/defra-node/src/assemble.rs assemble; crates/entry-points/defra-node/src/node_builder.rs NodeBuilder::with_identity_only",
        model_ref: "NodeSigningChoice (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A transaction the node begins and every request the node runs in it
        // agree on its owner: the node's principal, which is no one on an
        // identity-only node. The tests run that agreement on the live node.
        family: "Node identity and signing",
        name: "NodeSigningChoice.begin_and_execute_agree / owner_before_disagreed / default_owner_unchanged",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::node_principal, Node::node_context, Node::execute_prepared_request; crates/entry-points/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/entry-points/defra-node/src/ops/request.rs txn_owner",
        model_ref: "NodeSigningChoice (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/embedded_signer.rs an_identity_only_node_runs_requests_in_the_transactions_it_begins; crates/entry-points/defra-node/tests/nac_embedded_test.rs an_identity_only_node_is_checked_as_its_caller_under_nac",
    },
];
