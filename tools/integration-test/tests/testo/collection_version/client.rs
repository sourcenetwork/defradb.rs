//! Generated from the conformance scenarios of tests/integration/collection_version/client_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::Request;
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.builtin_types")]
fn introspection_explain_type_defined() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            Request {
                request: r#"
                    query {
                        __schema {
                            types {
                                kind
                                name
                                description
                            }
                        }
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__schema": m::object(json!({
                        "types": m::contains(json!([
                            {
                                "description": "ExplainType is an enum selecting the type of explanation done by the @explain directive.",
                                "kind": "ENUM",
                                "name": "ExplainType",
                            },
                        ])),
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
