//! Generated from the conformance scenarios of tests/integration/view/one_to_one/identical_schema_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, AddView, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.one_to_one")]
fn view_one_to_one_embedded_schema_is_not_lost_on_next_update() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        books: [Book]
                    }
                    type Book {
                        name: String
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Author {
                        name
                        books {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                        books: [BookView]
                    }
                    interface BookView {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.one_to_one")]
fn view_one_to_one_same_schema() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type LeftHand {
                        name: String
                        holding: RightHand @primary @relation(name: "left_right")
                        heldBy: RightHand @relation(name: "right_left")
                    }
                    type RightHand {
                        name: String
                        holding: LeftHand @primary @relation(name: "right_left")
                        heldBy: LeftHand @relation(name: "left_right")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    LeftHand {
                        name
                        heldBy {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type HandView @materialized(if: false) {
                        name: String
                        holding: HandView @primary
                        heldBy: HandView
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "LeftHand".into(),
                doc_map: Some(json!({ "name": "Left hand 1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "RightHand".into(),
                doc_map: Some(json!({ "_holdingID": m::r("doc0_0"), "name": "Right hand 1" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        HandView {
                            name
                            heldBy {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "HandView": [{ "heldBy": { "name": "Right hand 1" }, "name": "Left hand 1" }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
