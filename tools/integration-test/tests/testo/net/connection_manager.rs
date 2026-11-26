//! A node pushed over its connection manager's high water trims its oldest
//! peers and then settles (#3357): neither end's Kademlia redials a pruned
//! peer for the silence period after the trim, so the next prune pass finds
//! the node at low water and closes nothing.

use integration_test::actions::{
    Actions, ActivePeers, ConnectPeers, ConnectionManagerSpec, NewNode, PeerInfo, Wait,
    WaitForPeersEvents,
};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

const TARGET: &str = "db0";
const PEERS: [&str; 4] = ["db1", "db2", "db3", "db4"];
const GRACE_PERIOD_MS: u64 = 200;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// The target's active peers are the two it kept: the newest two.
fn kept_peers() -> Actions {
    ActivePeers {
        expected_data: json!({ "addresses": m::unordered(json!([m::r("peer3"), m::r("peer4")])) }),
        common: on(TARGET),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.net.peers.connection_manager_settles")]
fn net_connection_manager_pruned_peers_stay_disconnected() {
    let mut actions: Vec<Actions> = vec![NewNode {
        network: true,
        connection_manager: Some(ConnectionManagerSpec {
            low_water: 2,
            high_water: 3,
            grace_period_ms: GRACE_PERIOD_MS,
        }),
        common: on(TARGET),
        ..Default::default()
    }
    .into()];
    for (index, peer) in (1..).zip(PEERS) {
        actions.push(
            NewNode {
                network: true,
                common: on(peer),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            PeerInfo {
                common: Common {
                    node: Some(peer.into()),
                    capture: captures([(format!("peer{index}").as_str(), "/addresses/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        WaitForPeersEvents {
            start: true,
            common: on(TARGET),
            ..Default::default()
        }
        .into(),
    );
    // One at a time, each past the grace period before the next, so the
    // trim's choice of the two oldest is fixed.
    for peer in PEERS {
        actions.push(
            ConnectPeers {
                target: TARGET.into(),
                common: on(peer),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            Wait {
                millis: Some(GRACE_PERIOD_MS + 75),
                common: on(TARGET),
                ..Default::default()
            }
            .into(),
        );
    }
    // The host checks its watermarks every five seconds; the trim closes
    // the two oldest peers.
    actions.push(
        WaitForPeersEvents {
            disconnected: Some(vec!["db1".into(), "db2".into()]),
            millis: Some(10_000),
            common: on(TARGET),
            ..Default::default()
        }
        .into(),
    );
    actions.push(kept_peers());
    // The observation window: past the next prune pass and inside the 10s
    // silence period. Before #3357 Kademlia's bootstrap redialed the pruned
    // peers about 500ms after the trim, and that pass trimmed again.
    actions.push(
        Wait {
            millis: Some(6_000),
            common: on(TARGET),
            ..Default::default()
        }
        .into(),
    );
    actions.push(kept_peers());

    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        without_db: true,
        ..Default::default()
    }
    .execute();
}
