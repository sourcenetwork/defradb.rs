//! Generated from the conformance scenarios of tests/integration/view/one_to_many/with_transform_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddDoc, AddLens, AddView, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.transforms")]
fn view_one_to_many_with_transform_adding_inner_docs() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({
                            "dst": "books",
                            "value": [
                                { "name": "The Tragedy of Sohrab and Rostam" },
                                { "name": "The Legend of Seyavash" },
                            ],
                        }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Author {
                        name
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
                transform_cid: Some(Text::concat([
                    Text::from(""),
                    Text::reference("lens0"),
                    Text::from(""),
                ])),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Ferdowsi" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        AuthorView {
                            name
                            books {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "AuthorView": [
                        {
                            "books": [
                                { "name": "The Tragedy of Sohrab and Rostam" },
                                { "name": "The Legend of Seyavash" },
                            ],
                            "name": "Ferdowsi",
                        },
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

#[test]
#[mantra_macros::req_test("go.view.transforms")]
fn view_one_to_many_with_transform_on_outer() {
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
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "fullName", "src": "name" }),
                        path: "bin/lenses/copy.wasm".into(),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
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
                        fullName: String
                        books: [BookView]
                    }
                    interface BookView {
                        name: String
                    }
                "#
                .into(),
                transform_cid: Some(Text::concat([
                    Text::from(""),
                    Text::reference("lens0"),
                    Text::from(""),
                ])),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Ferdowsi" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "name": "Shahnameh" })),
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
                        AuthorView {
                            fullName
                            books {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "AuthorView": [{ "books": [{ "name": "Shahnameh" }], "fullName": "Ferdowsi" }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
