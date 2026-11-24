use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "HTTP client-signed conditional update",
        name: "accepted_filter_has_approved_target",
        axis: Lean,
        anchor: "crates/surfaces/http/src/client_signed_update.rs single_document_target",
        model_ref: "ClientSignedUpdateFilter (lake build); single_document_filter_admission_matches_model",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/http/tests/unit/client_signed_update.rs single_document_filter_admission_matches_model",
    },
];
