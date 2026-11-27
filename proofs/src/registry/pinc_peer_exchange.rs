use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A node that names hubs signs its record only once pinc settles it on
        // a relay; the exchange hands over addresses so peers meet before that.
        family: "pinc peer exchange",
        name: "LIVE_PeersMeet / LIVE_RecordsKept: two peers of one node meet whatever the order of their connects and records, and the node gets both records",
        axis: Tla,
        anchor: "crates/database/p2p/src/pinc/exchange.rs Node::share_peers, Node::learn_peers; crates/database/p2p/src/pinc/serve.rs Node::send_record, once_settled; crates/database/p2p/src/pinc/endpoint.rs Node::connected",
        model_ref: "MC_PincPeerExchange_{Green,Red_RecordOnly,Red_NoRetry}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/pinc/tests/exchange.rs two_peers_of_one_node_connect_before_their_records_are_signed",
    },
];
