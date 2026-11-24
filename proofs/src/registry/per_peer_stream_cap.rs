use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Per-peer stream cap",
        name: "INV_WithinCap / INV_SlotsFollowStreams / INV_ForgottenAreIdle",
        axis: Tla,
        anchor: "crates/database/p2p/src/two_stream/peer_streams.rs PeerStreams::admit, StreamAdmission; crates/database/p2p/src/two_stream/runner.rs TwoStreamRunner::run",
        model_ref: "MC_PeerStreamCap_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Replies to this node's own requests arrive as inbound streams and
        // share the per-peer cap and the max_p2p_tasks pool with requests, as
        // in Go's rcmgr. A reply past the cap is reset and shows up as a reply
        // timeout. libp2p only.
        family: "Per-peer stream cap",
        name: "INV_WithinCap counts replies with requests: a reply past the cap is reset",
        axis: Tla,
        anchor: "crates/database/p2p/src/two_stream/runner.rs TwoStreamRunner::run; crates/database/p2p/src/two_stream/peer_streams.rs PeerStreams::admit",
        model_ref: "MC_PeerStreamCap_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
