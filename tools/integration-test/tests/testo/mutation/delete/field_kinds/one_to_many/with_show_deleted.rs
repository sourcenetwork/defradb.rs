//! Generated from the conformance scenarios of tests/integration/mutation/delete/field_kinds/one_to_many/with_show_deleted_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.delete.relation")]
fn deletion_of_a_document_using_single_doc_id_with_show_deleted_document_query() {
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
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "name": "John and the philosopher are stoned",
                    "rating": 9.9,
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "name": "John has a chamber of secrets",
                    "rating": 9.9,
                })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"mutation {
					delete_Book(docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#"") {
							_docID
						}
					}"#,
                    ),
                ]),
                expected_data: json!({ "delete_Book": [{ "_docID": m::r("doc0_0") }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Author(showDeleted: true) {
                                                _deleted
                                                name
                                                age
                                                published {
                                                    _deleted
                                                    name
                                                    rating
                                                }
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "_deleted": false,
                            "age": 30,
                            "name": "John",
                            "published": [
                                {
                                    "_deleted": true,
                                    "name": "John and the philosopher are stoned",
                                    "rating": 9.9,
                                },
                                {
                                    "_deleted": false,
                                    "name": "John has a chamber of secrets",
                                    "rating": 9.9,
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    unordered_documents: vec!["/data/Author".into()],
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
