//! Generated from the conformance scenarios of tests/integration/collection_version/type_explicit_fields_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.mutation_inputs")]
fn encrypt_fields_for_add_mutation() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age:  Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    {
                      __type(name: "UserField") {
                        name
                        kind
                        enumValues {
                          name
                        }
                      }
                    }
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "enumValues": m::contains(json!([{ "name": "name" }, { "name": "age" }])),
                        "kind": "ENUM",
                        "name": "UserField",
                    })),
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
