//! A write's own `enableSigning: true` signs on a node that holds a key but
//! does not sign, as Go's `EnableSigning` option does
//! (`defra.signature.per_op_enable`).

use integration_test::actions::{AddCollection, AddDoc, NewNode, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.signature.per_op_enable")]
fn signature_with_per_op_signing_enabled_on_a_node_not_signing_signs_every_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                node_identity_seed: Some(0),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                enable_signing: Some(true),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            fieldName
                            signature {
                                type
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        { "fieldName": "age", "signature": { "type": "ES256K" } },
                        { "fieldName": "name", "signature": { "type": "ES256K" } },
                        { "fieldName": "_C", "signature": { "type": "ES256K" } },
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
