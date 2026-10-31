//! Generated from the conformance scenarios of tests/integration/mutation/delete/field_kinds/one_to_one_to_one/with_id_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.delete.relation")]
fn relational_deletion_of_a_document_using_single_key_success() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                        publisher: Publisher @primary
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        wrote: Book @primary
                    }

                    type Publisher {
                        name: String
                        address: String
                        published: Book
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "name": "Manning Early Access Program (MEAP)", "address": "Online" }),
                ),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "100 Go Mistakes to Avoid.",
                    "rating": 4.8,
                    "_publisherID": m::r("doc2_0"),
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "name": "Teiva Harsanyi",
                    "age": 48,
                    "verified": true,
                    "_wroteID": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"mutation {
						delete_Author(docID: ""#,
                    ),
                    Text::reference("doc1_0"),
                    Text::from(
                        r#"") {
							_docID
						}
					}"#,
                    ),
                ]),
                expected_data: json!({ "delete_Author": [{ "_docID": m::r("doc1_0") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.delete.relation")]
fn relational_deletion_of_a_document_using_single_key_with_alias_success() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                        publisher: Publisher @primary
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        wrote: Book @primary
                    }

                    type Publisher {
                        name: String
                        address: String
                        published: Book
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "name": "Manning Early Access Program (MEAP)", "address": "Online" }),
                ),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "100 Go Mistakes to Avoid.",
                    "rating": 4.8,
                    "_publisherID": m::r("doc2_0"),
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "name": "Teiva Harsanyi",
                    "age": 48,
                    "verified": true,
                    "_wroteID": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"mutation {
						delete_Author(docID: ""#,
                    ),
                    Text::reference("doc1_0"),
                    Text::from(
                        r#"") {
							AliasOfKey: _docID
						}
					}"#,
                    ),
                ]),
                expected_data: json!({ "delete_Author": [{ "AliasOfKey": m::r("doc1_0") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.delete.relation")]
fn relational_deletion_of_a_document_using_single_key_with_multi_documents_with_alias_success() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                        publisher: Publisher @primary
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        wrote: Book @primary
                    }

                    type Publisher {
                        name: String
                        address: String
                        published: Book
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "name": "Manning Early Access Program (MEAP)", "address": "Online" }),
                ),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "100 Go Mistakes to Avoid.",
                    "rating": 4.8,
                    "_publisherID": m::r("doc2_0"),
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "name": "Teiva Harsanyi",
                    "age": 48,
                    "verified": true,
                    "_wroteID": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Manning Publications", "address": "Website" })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Author".into(),
                doc_id: Text::reference("doc1_0"),
                doc_map: json!({ "name": "Teiva Harsanyiiiiiiiiii", "age": 49 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"mutation {
						delete_Author(docID: ""#,
                    ),
                    Text::reference("doc1_0"),
                    Text::from(
                        r#"") {
							Key: _docID
						}
					}"#,
                    ),
                ]),
                expected_data: json!({ "delete_Author": [{ "Key": m::r("doc1_0") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
