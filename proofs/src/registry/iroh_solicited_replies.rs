use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Iroh solicited replies",
        name: "INV_NoUnsolicitedAdmitted / INV_SolicitedAdmitted / INV_NoReplyStreamOnV2",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/solicited.rs Solicited::expect, Solicited::take, Solicited::prune; crates/database/p2p/src/iroh/endpoint_streams.rs admit_stream, dispatch_stream; crates/database/p2p/src/iroh/endpoint_commands.rs handle_command; crates/database/p2p/src/iroh/endpoint_rpc.rs handle_reply_on_stream",
        model_ref: "MC_SolicitedReplies_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Reply expectations are keyed by (peer, tag) across all of the peer's
        // connections, and are taken before the size, budget and body checks.
        // A concurrent mux 0.1 connection can therefore open up to that many
        // unowed reply streams (still budgeted and matched by message id), and
        // a reply reset for room or size loses its expectation.
        family: "Iroh solicited replies",
        name: "INV_NoUnsolicitedAdmitted counts expectations per (peer, tag), not per connection or reply",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/solicited.rs Solicited::expect, Solicited::take; crates/database/p2p/src/iroh/endpoint_streams.rs admit_stream",
        model_ref: "MC_SolicitedReplies_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
