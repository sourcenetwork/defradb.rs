//! Generated from the conformance scenarios of tests/integration/mutation/add/simple_add_many_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.add.basic")]
fn mutation_add_many() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
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
                docs: Some(vec![
                    json!({ "name": "John", "age": 27 }),
                    json!({ "name": "Islam", "age": 33 }),
                ]),
                common: Common {
                    capture: captures([("doc0", "/docIDs/0"), ("doc1", "/docIDs/1")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "_docID": m::r("doc0"), "age": 27, "name": "John" },
                        { "_docID": m::r("doc1"), "age": 33, "name": "Islam" },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
