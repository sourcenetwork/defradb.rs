//! Generated from the conformance scenarios of tests/integration/mutation/add/with_version_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.add.basic")]
fn mutation_add_returns_version_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                                add_Users(input: {name: "John"}) {
                                                    _version {
                                                        cid
                                                    }
                                                }
                                            }
                "#
                .into(),
                expected_data: json!({ "add_Users": [{ "_version": [{ "cid": m::cid() }] }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
