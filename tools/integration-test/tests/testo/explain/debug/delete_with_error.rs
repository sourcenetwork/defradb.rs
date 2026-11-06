//! Generated from the conformance scenarios of tests/integration/explain/debug/delete_with_error_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn debug_explain_mutation_request_with_delete_having_no_sub_selection() {
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
            Request {
                request: r#"
                    mutation @explain(type: debug) {
                                        delete_Author(
                                            docID: [
                                                "bae-079d0bd8-4b1b-5f5f-bd95-4d915c277f9d",
                                                "bae-bfbfc89c-0d63-5ea4-81a3-3ebd295be67f"
                                            ]
                                        )
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Field "delete_Author" of type "[Author]" must have a sub selection."#.into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
