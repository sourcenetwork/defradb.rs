//! Generated from the conformance scenarios of tests/integration/node/identity_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{GetNodeIdentity, NewNode};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.node.identity")]
fn node_identity_node_identity_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                node_identity_seed: Some(0),
                network: true,
                with_identity: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(1),
                network: true,
                with_identity: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            GetNodeIdentity {
                expected_data: json!({
                    "identity": {
                        "PublicKey": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                        "DID": "did:key:z7r8ory9gZDehP2LqJPZxjSKLp3aHZKsgdFMmuyRWKVFWPe8HR8cmgew2q8sQe3D5PrxGJKJSFHFcH3wKvEBzVWjSjd9b",
                    },
                }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            GetNodeIdentity {
                expected_data: json!({
                    "identity": {
                        "PublicKey": "0271cad82a372e078b1e8f2d32cc9f325a7425718bdf2f7288f3809105649dfae9",
                        "DID": "did:key:z7r8oqkfiiVe4bHLYBjHZTJqGiUqCuMo6q7qiNGNYogBb8CZhDZ6RmFocZYYrsxCLew1E9bdWJ5tC7bVCGosfQDrSy7nf",
                    },
                }),
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
