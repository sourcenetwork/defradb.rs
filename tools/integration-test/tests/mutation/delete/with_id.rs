//! Generated from the conformance scenarios of tests/integration/mutation/delete/with_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.delete.by_id")]
fn mutation_deletion_with_id_unknown_value_and_unrelated_record_in_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Not the id below" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        delete_User(docID: ["bae-390b4419-fe1c-506b-98bd-20847cdab2d9"]) {
                                            _docID
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "delete_User": [] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.delete.by_id")]
fn mutation_deletion_with_id_unknown_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        delete_User(docID: ["bae-390b4419-fe1c-506b-98bd-20847cdab2d9"]) {
                                            _docID
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "delete_User": [] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
