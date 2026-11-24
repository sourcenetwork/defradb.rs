//! A libp2p node whose host resolver configuration lists no nameservers still
//! starts, as Go's does: its DNS layers fall back to the local nameservers.

use integration_test::actions::{NewNode, PeerInfo, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.node.start.no_nameservers")]
fn a_libp2p_node_starts_when_resolv_conf_lists_no_nameservers() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                resolv_conf: Some(String::new()),
                ..Default::default()
            }
            .into(),
            PeerInfo {
                expected_data: json!({ "addresses": m::nonempty() }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { __schema { queryType { name } } }".into(),
                expected_data: json!({ "__schema": { "queryType": { "name": "Query" } } }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.node.start.no_nameservers")]
fn a_libp2p_node_starts_when_resolv_conf_does_not_parse() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                resolv_conf: Some("nameserver not-an-ip\n".into()),
                ..Default::default()
            }
            .into(),
            PeerInfo {
                expected_data: json!({ "addresses": m::nonempty() }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { __schema { queryType { name } } }".into(),
                expected_data: json!({ "__schema": { "queryType": { "name": "Query" } } }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
