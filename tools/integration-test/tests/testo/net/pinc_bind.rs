//! Where an embedder's pinc endpoint binds when it names no bind.

use integration_test::actions::{NewNode, PeerInfo};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

/// A node keeping `defra-node`'s default bind binds loopback, as `defradb
/// start` does: its peer info lists a loopback address, which a node bound
/// on every interface never lists.
#[test]
#[mantra_macros::req_test("defra.cli.pinc.embedder_bind")]
fn a_node_that_names_no_bind_binds_loopback() {
    TestCase::from_actions(vec![
        NewNode {
            network: true,
            default_pinc_bind: true,
            ..Default::default()
        }
        .into(),
        PeerInfo {
            expected_data: json!({ "addresses": [m::substring("@127.0.0.1:")] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
