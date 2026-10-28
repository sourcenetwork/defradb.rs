//! Generated from the conformance scenarios of tests/integration/mutation/update/with_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.update.by_filter")]
fn mutation_update_with_boolean_filter_result_not_filtered_out() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        verified: Boolean
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "verified": true })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        update_Users(filter: {verified: {_eq: true}}, input: {verified: false}) {
                                            name
                                            verified
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "update_Users": [{ "name": "John", "verified": false }] }),
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
#[mantra_macros::req_test("go.mutation.update.by_filter")]
fn mutation_update_with_boolean_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        verified: Boolean
                        points: Float
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "verified": true, "points": 42.1 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Bob", "verified": false, "points": 66.6 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "verified": true, "points": 33 })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        update_Users(filter: {verified: {_eq: true}}, input: {points: 59}) {
                                            name
                                            points
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "update_Users": [
                        { "name": "John", "points": 59.0 },
                        { "name": "Fred", "points": 59.0 },
                    ],
                }),
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
#[mantra_macros::req_test("go.mutation.update.by_filter")]
fn mutation_update_with_filter_on_updated_field_returns_result() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        update_Users(filter: {name: {_eq: "John"}}, input: {name: "Jane"}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "update_Users": [{ "name": "Jane" }] }),
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
#[mantra_macros::req_test("go.mutation.update.by_filter")]
fn mutation_update_with_relation_filter_and_relation_select_returns_both() {
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
                        published: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Book(input: {name: "Painted House", rating: 4.9}) { _docID } }"#.into(),
                common: Common { capture: captures([("doc0_0", "/data/add_Book/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Book(input: {name: "Boring Book", rating: 2}) { _docID } }"#.into(),
                common: Common { capture: captures([("doc0_1", "/data/add_Book/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"mutation { add_Author(input: {_publishedID: ""#), Text::reference("doc0_0"), Text::from(r#"", name: "John Grisham"}) { _docID } }"#)]),
                common: Common { capture: captures([("doc1_0", "/data/add_Author/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"mutation { add_Author(input: {_publishedID: ""#), Text::reference("doc0_1"), Text::from(r#"", name: "Bad Writer"}) { _docID } }"#)]),
                common: Common { capture: captures([("doc1_1", "/data/add_Author/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        update_Author(
                                            filter: {published: {rating: {_gt: 3}}},
                                            input: {name: "Jane Grisham"}
                                        ) {
                                            name
                                            published {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "update_Author": [
                        {
                            "name": "Jane Grisham",
                            "published": { "name": "Painted House" },
                        },
                    ],
                }),
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
#[mantra_macros::req_test("go.mutation.update.by_filter")]
fn mutation_update_with_relation_filter_correctly_filters() {
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
                        published: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Book(input: {name: "Painted House", rating: 4.9}) { _docID } }"#.into(),
                common: Common { capture: captures([("doc0_0", "/data/add_Book/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Book(input: {name: "Boring Book", rating: 2}) { _docID } }"#.into(),
                common: Common { capture: captures([("doc0_1", "/data/add_Book/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"mutation { add_Author(input: {_publishedID: ""#), Text::reference("doc0_0"), Text::from(r#"", name: "John Grisham"}) { _docID } }"#)]),
                common: Common { capture: captures([("doc1_0", "/data/add_Author/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"mutation { add_Author(input: {_publishedID: ""#), Text::reference("doc0_1"), Text::from(r#"", name: "Bad Writer"}) { _docID } }"#)]),
                common: Common { capture: captures([("doc1_1", "/data/add_Author/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        update_Author(
                                            filter: {published: {rating: {_gt: 3}}},
                                            input: {name: "Jane Grisham"}
                                        ) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "update_Author": [{ "name": "Jane Grisham" }] }),
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
#[mantra_macros::req_test("go.mutation.update.by_filter")]
fn mutation_update_with_relation_select_in_response_returns_relation() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }

                    type Author {
                        name: String
                        published: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Book(input: {name: "Painted House"}) { _docID } }"#.into(),
                common: Common { capture: captures([("doc0_0", "/data/add_Book/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"mutation { add_Author(input: {_publishedID: ""#), Text::reference("doc0_0"), Text::from(r#"", name: "John Grisham"}) { _docID } }"#)]),
                common: Common { capture: captures([("doc1_0", "/data/add_Author/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation($docID: [ID!]) {
                                        update_Author(docID: $docID, input: {name: "Jane Grisham"}) {
                                            name
                                            published {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                variables: Some(json!({ "docID": m::r("doc1_0") })),
                expected_data: json!({
                    "update_Author": [
                        {
                            "name": "Jane Grisham",
                            "published": { "name": "Painted House" },
                        },
                    ],
                }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
