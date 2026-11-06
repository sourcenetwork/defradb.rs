//! Generated from the conformance scenarios of tests/integration/explain/execute/dagscan_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.execute.dagscan")]
fn execute_explain_commits_dag_scan() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Article {
                        name: String
                        author: Author
                        pages: Int
                    }

                    type Book {
                        name: String
                        author: Author
                        rating: Float
                        pages: Int
                        chapterPages: [Int!]
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        books: [Book]
                        articles: [Article]
                        contact: AuthorContact @primary
                    }

                    type AuthorContact {
                        cell: String
                        email: String
                        author: Author
                        address: ContactAddress @primary
                    }

                    type ContactAddress {
                        city: String
                        country: String
                        contact: AuthorContact
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "ContactAddress".into(),
                doc_map: Some(json!({ "city": "Waterloo", "country": "Canada" })),
                common: Common {
                    capture: captures([("doc4_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "ContactAddress".into(),
                doc_map: Some(json!({ "city": "Brampton", "country": "Canada" })),
                common: Common {
                    capture: captures([("doc4_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query @explain(type: execute) {
					_commits (docID: ""#,
                    ),
                    Text::reference("doc4_0"),
                    Text::from(
                        r#"") {
						links {
							cid
						}
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "dagScanNode": { "iterations": 4 },
                                        "filterMatches": 3,
                                        "iterations": 4,
                                    },
                                },
                            },
                        ],
                        "planExecutions": 2,
                        "sizeOfResult": 1,
                    },
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
