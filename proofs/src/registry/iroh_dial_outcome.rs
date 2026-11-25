use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // An iroh dial answers from its connection's outcome: a refusal that
        // already closed the connection is answered Unauthorized, and
        // connect_peer ends with the dial's answer instead of polling the
        // connected set until a timeout.
        family: "Iroh dial outcome",
        name: "INV_RefusalKnownIsSaid / LIVE_DialAnswered: a dial never answers ok on a connection the peer's refusal already closed, and every dial is answered",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_commands.rs handle_dial; crates/surfaces/p2p-adapter/src/iroh.rs IrohP2PAdapter::connect_peer; crates/database/p2p/src/iroh/transport.rs poll_until_connected",
        model_ref: "MC_IrohDialOutcome_{Green,Green_Admitted,Red_Poll,Red_NoCheck}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/iroh/allowlist_tests.rs closed_by_refusal_reads_the_remote_close_code, poll_until_connected_wakes_on_the_connection_and_times_out_without_one",
    },
];
