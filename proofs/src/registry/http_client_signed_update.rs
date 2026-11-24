use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "http, query, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "HTTP client-signed update",
        name: "fixed intent, live signing, relevant field and deciding authorization",
        axis: Tla,
        anchor: "crates/surfaces/http/src/client_signing.rs run; crates/database/db/src/block/builder/client_update.rs and update transactions",
        model_ref: "MC_ClientSignedUpdate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/http_signing_tests.rs http_client_signs_a_single_document_update, http_client_signed_update_rechecks_acp_and_cancellation",
    },
    Property {
        family: "HTTP client-signed update",
        name: "signed_document / signed_changes / signed_exact_wave / unchanged_skipped / immutable_refused / mutable_allowed",
        axis: Lean,
        anchor: "crates/database/db/src/block/builder/client_update.rs; crates/database/db/src/block/builder/client_waves.rs ClientWaves::respond",
        model_ref: "ClientSignedUpdate (lake build); client_create_graph::trusted_update_graph_matches_model_and_rejects_unapproved_waves",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/block/client_create_graph.rs trusted_update_graph_matches_model_and_rejects_unapproved_waves",
    },
];
