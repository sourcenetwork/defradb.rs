use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "http, defra-node";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "HTTP router composition",
        name: "WholeRouter / SingleConstructor / NoBypass / ForwardingPreserved",
        axis: Tla,
        anchor: "crates/surfaces/http/src/server.rs Server::router, Server::serve; router_transform_tests.rs forwarding_filter_matches_executed_model; crates/entry-points/defra-node/src/router_transform_tests.rs",
        model_ref: "MC_HttpRouterComposition_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "HTTP router composition",
        name: "configured_order / constructor_once / absent_preserves / forwarding_preserves / refuses_every_inner_status",
        axis: Lean,
        anchor: "crates/surfaces/http/src/server.rs Server::router; router_transform_tests.rs forwarding_filter_matches_executed_model",
        model_ref: "HttpRouterComposition (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
