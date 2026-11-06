//! Generated from the conformance scenarios of tests/integration/mutation/delete/field_kinds/one_to_one_to_one/with_txn_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, TxnBegin, TxnCommit, TxnDiscard};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.txn.isolation")]
fn a_txn_can_read_a_record_that_is_deleted_in_a_non_commited_txn_backward_direction() {
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
                doc_map: Some(json!({ "name": "Website", "address": "Manning Publications" })),
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
                    "name": "Book By Website",
                    "rating": 4.0,
                    "_publisherID": m::r("doc2_0"),
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
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
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(1),
                request: r#"
                    query {
                                        Book {
                                            _docID
                                            name
                                            publisher {
                                                _docID
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "_docID": m::r("doc0_0"),
                            "name": "Book By Website",
                            "publisher": { "_docID": m::r("doc2_0"), "name": "Website" },
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            _docID
                                            name
                                            publisher {
                                                _docID
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Book": [] }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 1,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.txn.isolation")]
fn a_txn_can_read_a_record_that_is_deleted_in_a_non_commited_txn_forward_direction() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Website", "address": "Manning Publications" })),
                common: Common { capture: captures([("doc2_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "Book By Website",
                    "rating": 4.0,
                    "_publisherID": m::r("doc2_0"),
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: Text::concat([Text::from(r#"mutation {
			        delete_Book(docID: ""#), Text::reference("doc0_0"), Text::from(r#"") {
			            _docID
			        }
			    }"#)]),
                expected_data: json!({ "delete_Book": [{ "_docID": m::r("doc0_0") }] }),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(1),
                request: r#"
                    query {
                                        Publisher {
                                            _docID
                                            name
                                            published {
                                                _docID
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Publisher": [
                        {
                            "_docID": m::r("doc2_0"),
                            "name": "Website",
                            "published": { "_docID": m::r("doc0_0"), "name": "Book By Website" },
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Publisher {
                                            _docID
                                            name
                                            published {
                                                _docID
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Publisher": [{ "_docID": m::r("doc2_0"), "name": "Website", "published": null }],
                }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 1,
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
fn txn_deletion_of_related_doc_from_non_primary_side_backward_direction() {
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
                    json!({ "name": "Online", "address": "Manning Early Access Program (MEAP)" }),
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
                    "name": "Book By Online",
                    "rating": 4.0,
                    "_publisherID": m::r("doc2_0"),
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: Text::concat([
                    Text::from(
                        r#"mutation {
					delete_Publisher(docID: ""#,
                    ),
                    Text::reference("doc2_0"),
                    Text::from(
                        r#"") {
			            _docID
			        }
			    }"#,
                    ),
                ]),
                expected_data: json!({ "delete_Publisher": [{ "_docID": m::r("doc2_0") }] }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            _docID
                                            name
                                            publisher {
                                                _docID
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "_docID": m::r("doc0_0"),
                            "name": "Book By Online",
                            "publisher": null,
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
#[mantra_macros::req_test("go.mutation.delete.relation")]
fn txn_deletion_of_related_doc_from_non_primary_side_forward_direction() {
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
                    json!({ "name": "Online", "address": "Manning Early Access Program (MEAP)" }),
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
                    "name": "Book By Online",
                    "rating": 4.0,
                    "_publisherID": m::r("doc2_0"),
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: Text::concat([
                    Text::from(
                        r#"mutation {
					delete_Publisher(docID: ""#,
                    ),
                    Text::reference("doc2_0"),
                    Text::from(
                        r#"") {
			            _docID
			        }
			    }"#,
                    ),
                ]),
                expected_data: json!({ "delete_Publisher": [{ "_docID": m::r("doc2_0") }] }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Publisher {
                                            _docID
                                            name
                                            published {
                                                _docID
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Publisher": [] }),
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
fn txn_deletion_of_related_doc_from_primary_side_backward_direction() {
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
                doc_map: Some(json!({ "name": "Website", "address": "Manning Publications" })),
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
                    "name": "Book By Website",
                    "rating": 4.0,
                    "_publisherID": m::r("doc2_0"),
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
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
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            _docID
                                            name
                                            publisher {
                                                _docID
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Book": [] }),
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
fn txn_deletion_of_related_doc_from_primary_side_forward_direction() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Website", "address": "Manning Publications" })),
                common: Common { capture: captures([("doc2_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "Book By Website",
                    "rating": 4.0,
                    "_publisherID": m::r("doc2_0"),
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: Text::concat([Text::from(r#"mutation {
			        delete_Book(docID: ""#), Text::reference("doc0_0"), Text::from(r#"") {
			            _docID
			        }
			    }"#)]),
                expected_data: json!({ "delete_Book": [{ "_docID": m::r("doc0_0") }] }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Publisher {
                                            _docID
                                            name
                                            published {
                                                _docID
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Publisher": [{ "_docID": m::r("doc2_0"), "name": "Website", "published": null }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
