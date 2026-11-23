use integration_test::actions::{
    ActivePeers, AddCollection, AddDoc, AddReplicator, ConnectPeers, DisconnectPeers, NewNode,
    PeerInfo, Wait,
};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.net.peers.disconnect")]
fn net_info_disconnect_sticks_until_explicit_reconnect() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
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
            DisconnectPeers {
                targets: vec!["db0".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(500),
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
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.net.peers.sticky_disconnect")]
fn a_replicator_retry_does_not_redial_a_disconnected_peer() {
    let on = |node: &str| Common {
        node: Some(node.into()),
        ..Default::default()
    };
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        without_db: true,
        actions: vec![
            NewNode {
                network: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Note { text: String }".into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                collections: vec!["Note".into()],
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            DisconnectPeers {
                targets: vec!["db1".into()],
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Note".into(),
                doc_map: Some(json!({"text": "pending"})),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(8000),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            ActivePeers {
                expected_data: json!({"addresses": m::unordered(json!([]))}),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                collections: vec!["Note".into()],
                common: on("db0"),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
