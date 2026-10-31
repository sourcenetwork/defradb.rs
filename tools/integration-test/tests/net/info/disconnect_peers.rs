//! Generated from the conformance scenarios of tests/integration/net/info/disconnect_peers_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{ActivePeers, ConnectPeers, DisconnectPeers, NewNode, PeerInfo};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.net.peers.disconnect")]
fn net_info_disconnect_empty_list() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerInfo {
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("peer0", "/addresses/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DisconnectPeers {
                targets: vec![],
                expected_error: Some("addresses cannot be empty".into()),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.net.peers.disconnect")]
fn net_info_disconnect_multiple_peers() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerInfo {
                common: Common { node: Some("db0".into()), capture: captures([("peer0", "/addresses/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerInfo {
                common: Common { node: Some("db1".into()), capture: captures([("peer1", "/addresses/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerInfo {
                common: Common { node: Some("db2".into()), capture: captures([("peer2", "/addresses/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db2".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ActivePeers {
                expected_data: json!({ "addresses": m::unordered(json!([m::r("peer0"), m::r("peer2")])) }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            DisconnectPeers {
                targets: vec!["db0".into(), "db2".into()],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ActivePeers {
                expected_data: json!({ "addresses": m::unordered(json!([])) }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.net.peers.disconnect")]
fn net_info_disconnect_single_peer() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerInfo {
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("peer0", "/addresses/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerInfo {
                common: Common {
                    node: Some("db1".into()),
                    capture: captures([("peer1", "/addresses/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ActivePeers {
                expected_data: json!({ "addresses": m::unordered(json!([m::r("peer0")])) }),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DisconnectPeers {
                targets: vec!["db0".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ActivePeers {
                expected_data: json!({ "addresses": m::unordered(json!([])) }),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
