//! Generated from the conformance scenarios of tests/integration/explain/default/group_with_doc_id_child_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.group.selection")]
fn default_explain_request_with_doc_i_ds_on_inner_group_selection() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain {
                                        Author(
                                            groupBy: [age]
                                        ) {
                                            age
                                            GROUP(docID: ["bae-6a4c5bc5-b044-5a03-a868-8260af6f2254"]) {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": { "groupNode": { "selectNode": { "scanNode": {} } } },
                                },
                            ],
                        },
                    })),
                    m::explain_target("groupNode", false, 0, json!({
                        "childSelects": [
                            {
                                "collectionName": "Author",
                                "docID": ["bae-6a4c5bc5-b044-5a03-a868-8260af6f2254"],
                                "filter": null,
                                "groupBy": null,
                                "limit": null,
                                "orderBy": null,
                            },
                        ],
                        "groupByFields": ["age"],
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": null,
                        "prefixes": ["/3"],
                    })),
                ]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
