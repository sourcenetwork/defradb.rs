//! Generated from the conformance scenarios of tests/integration/query/one_to_many/with_cid_doc_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many.time-travel")]
fn query_one_to_many_with_child_update_and_first_cid_and_doc_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Author(docID: ""#),
                    Text::reference("doc1_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID1_0_0", "/data/Author/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "name": "Painted House", "rating": 4.9, "_authorID": m::r("doc1_0") }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Book(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Book/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Author".into(),
                doc_id: Text::reference("doc1_0"),
                doc_map: json!({ "age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Author(docID: ""#),
                    Text::reference("doc1_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID1_0_1", "/data/Author/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Book (
							cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#"",
							docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
						) {
						name
						author {
							name
							age
						}
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "age": 22, "name": "John Grisham" },
                            "name": "Painted House",
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.time-travel")]
fn query_one_to_many_with_cid_and_doc_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Author(docID: ""#),
                    Text::reference("doc1_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID1_0_0", "/data/Author/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "name": "Painted House", "rating": 4.9, "_authorID": m::r("doc1_0") }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Book(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Book/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Book (
							cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#""
							docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
						) {
						name
						author {
							name
						}
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "Book": [{ "author": { "name": "John Grisham" }, "name": "Painted House" }],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.time-travel")]
fn query_one_to_many_with_parent_update_and_first_cid_and_doc_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Author(docID: ""#),
                    Text::reference("doc1_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID1_0_0", "/data/Author/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "name": "Painted House", "rating": 4.9, "_authorID": m::r("doc1_0") }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Book(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Book/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Book".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "rating": 4.5 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Book(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_1", "/data/Book/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Book (
						cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#"",
						docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
					) {
						name
						rating
						author {
							name
						}
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "name": "John Grisham" },
                            "name": "Painted House",
                            "rating": 4.9,
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.time-travel")]
fn query_one_to_many_with_parent_update_and_last_cid_and_doc_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Author(docID: ""#),
                    Text::reference("doc1_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID1_0_0", "/data/Author/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "name": "Painted House", "rating": 4.9, "_authorID": m::r("doc1_0") }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Book(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Book/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Book".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "rating": 4.5 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Book(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_1", "/data/Book/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Book (
						cid: ""#,
                    ),
                    Text::reference("CID0_0_1"),
                    Text::from(
                        r#"",
						docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
					) {
						name
						rating
						author {
							name
						}
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "name": "John Grisham" },
                            "name": "Painted House",
                            "rating": 4.5,
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
