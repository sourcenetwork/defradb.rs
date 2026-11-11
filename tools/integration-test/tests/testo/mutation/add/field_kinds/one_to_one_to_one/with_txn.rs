//! Generated from the conformance scenarios of tests/integration/mutation/add/field_kinds/one_to_one_to_one/with_txn_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, TxnBegin, TxnCommit};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::{NotApplying, TestCase};
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.txn.conflict")]
fn transactional_creation_and_linking_of_relational_documents_backward() {
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
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Online", "address": "Manning Early Access Program (MEAP)" })),
                common: Common { capture: captures([("doc2_1", "/docID")]), ..Default::default() },
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
					add_Book(input: {name: "Book By Website", rating: 4.0, _publisherID: ""#), Text::reference("doc2_0"), Text::from(r#""}) {
						_docID
					}
				}"#)]),
                expected_data: json!({ "add_Book": [{ "_docID": m::doc_id() }] }),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(1),
                request: Text::concat([Text::from(r#"mutation {
					add_Book(input: {name: "Book By Online", rating: 4.0, _publisherID: ""#), Text::reference("doc2_1"), Text::from(r#""}) {
						_docID
					}
				}"#)]),
                expected_data: json!({ "add_Book": [{ "_docID": m::doc_id() }] }),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
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
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "_docID": m::doc_id(),
                            "name": "Book By Website",
                            "publisher": { "_docID": m::r("doc2_0"), "name": "Website" },
                        },
                    ],
                }),
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
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "_docID": m::doc_id(),
                            "name": "Book By Online",
                            "publisher": { "_docID": m::r("doc2_1"), "name": "Online" },
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
            TxnCommit {
                txn: 1,
                expected_error: Some("transaction conflict".into()),
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
                    "Publisher": [
                        { "_docID": m::r("doc2_1"), "name": "Online", "published": null },
                        {
                            "_docID": m::r("doc2_0"),
                            "name": "Website",
                            "published": { "_docID": m::doc_id(), "name": "Book By Website" },
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/Publisher".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        not_applying: Some(NotApplying::GoBugSuspect(r#"Go fails the second commit with "transaction conflict", an SSI anti-dependency on the publisher each transaction read (go.mutation.txn.conflict). The transactions write disjoint books and publishers, so regolith commits both and both books link correctly."#)),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.txn.conflict")]
fn transactional_creation_and_linking_of_relational_documents_forward() {
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
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "name": "Online", "address": "Manning Early Access Program (MEAP)" }),
                ),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
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
					add_Book(input: {name: "Book By Website", rating: 4.0, _publisherID: ""#,
                    ),
                    Text::reference("doc2_0"),
                    Text::from(
                        r#""}) {
						_docID
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "add_Book": [{ "_docID": m::doc_id() }] }),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(1),
                request: Text::concat([
                    Text::from(
                        r#"mutation {
					add_Book(input: {name: "Book By Online", rating: 4.0, _publisherID: ""#,
                    ),
                    Text::reference("doc2_1"),
                    Text::from(
                        r#""}) {
						_docID
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "add_Book": [{ "_docID": m::doc_id() }] }),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
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
                expected_data: json!({
                    "Publisher": [
                        { "_docID": m::r("doc2_1"), "name": "Online", "published": null },
                        {
                            "_docID": m::r("doc2_0"),
                            "name": "Website",
                            "published": { "_docID": m::doc_id(), "name": "Book By Website" },
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Publisher".into()],
                    ..Default::default()
                },
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
                "#
                .into(),
                expected_data: json!({
                    "Publisher": [
                        {
                            "_docID": m::r("doc2_1"),
                            "name": "Online",
                            "published": { "_docID": m::doc_id(), "name": "Book By Online" },
                        },
                        { "_docID": m::r("doc2_0"), "name": "Website", "published": null },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Publisher".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 1,
                expected_error: Some("transaction conflict".into()),
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
                            "_docID": m::doc_id(),
                            "name": "Book By Website",
                            "publisher": { "_docID": m::r("doc2_0"), "name": "Website" },
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
