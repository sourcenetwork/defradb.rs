//! Explain puts each filter on the scan node of the join that applies it.
//! Rust adds index fields to a scan node, so Go is not run.

use integration_test::actions::{AddCollection, GetCollections, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

/// Captures the version ID of `collection` as `name`.
fn capture_version(collection: &str, name: &str) -> GetCollections {
    GetCollections {
        collection_name: Some(collection.into()),
        filtered: true,
        common: Common {
            capture: captures([(name, "/collections/0/VersionID")]),
            ..Default::default()
        },
        ..Default::default()
    }
}

const PERSON_INDEXED_BOOKS: &str = r#"
    type Person {
        name: String
        authoredBooks: [Book]
    }
    type Book {
        title: String @index
        pages: Int
        author: Person
    }
"#;

/// The explain of `Person` joined to its books: the books' select node holds
/// `select_filter` and reads through `book_scan`.
fn person_books_join(
    select_filter: serde_json::Value,
    book_scan: serde_json::Value,
) -> serde_json::Value {
    json!({
        "typeIndexJoin": {
            "joinType": "typeJoinMany",
            "root": {
                "scanNode": {
                    "collectionID": m::r("person_version"),
                    "collectionName": "Person",
                    "filter": null,
                    "prefixes": ["/1"],
                },
            },
            "rootName": "author",
            "subType": {
                "selectTopNode": {
                    "selectNode": {
                        "docID": null,
                        "filter": select_filter,
                        "scanNode": book_scan,
                    },
                },
            },
            "subTypeName": "authoredBooks",
        },
    })
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate_filter_own_join")]
fn an_indexed_child_and_an_aggregate_filter_each_keep_their_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        actions: vec![
        AddCollection {
            sdl: PERSON_INDEXED_BOOKS.into(),
            ..Default::default()
        }
        .into(),
        capture_version("Person", "person_version").into(),
        capture_version("Book", "book_version").into(),
        Request {
            request: r#"query @explain {
                Person {
                    name
                    authoredBooks(filter: {title: {_eq: "A"}}) { title }
                    COUNT(authoredBooks: {filter: {pages: {_gt: 100}}})
                }
            }"#
            .into(),
            expected_data: json!({
                "explain": {
                    "operationNode": [{
                        "selectTopNode": {
                            "countNode": {
                                "selectNode": {
                                    "docID": null,
                                    "filter": null,
                                    "parallelNode": [
                                        person_books_join(json!({ "title": { "_eq": "A" } }), json!({
                                            "collectionID": m::r("book_version"),
                                            "collectionName": "Book",
                                            "filter": { "title": { "_eq": "A" } },
                                            "indexFetches": 0,
                                            "indexLimit": null,
                                            "indexName": "Book_title_ASC",
                                            "indexOffset": 0,
                                            "prefixes": ["/2"],
                                        })),
                                        person_books_join(json!(null), json!({
                                            "collectionID": m::r("book_version"),
                                            "collectionName": "Book",
                                            "filter": { "pages": { "_gt": 100 } },
                                            "prefixes": ["/2"],
                                        })),
                                    ],
                                },
                                "sources": [{
                                    "fieldName": "authoredBooks",
                                    "filter": { "pages": { "_gt": 100 } },
                                }],
                            },
                        },
                    }],
                },
            }),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query @explain {
                Person {
                    name
                    authoredBooks(filter: {title: {_eq: "A"}}) { title }
                    COUNT(authoredBooks: {filter: {title: {_eq: "A"}}})
                }
            }"#
            .into(),
            expected_data: json!({
                "explain": {
                    "operationNode": [{
                        "selectTopNode": {
                            "countNode": {
                                "selectNode": {
                                    "docID": null,
                                    "filter": null,
                                    "typeIndexJoin": person_books_join(json!({ "title": { "_eq": "A" } }), json!({
                                        "collectionID": m::r("book_version"),
                                        "collectionName": "Book",
                                        "filter": { "title": { "_eq": "A" } },
                                        "indexFetches": 0,
                                        "indexLimit": null,
                                        "indexName": "Book_title_ASC",
                                        "indexOffset": 0,
                                        "prefixes": ["/2"],
                                    }))["typeIndexJoin"],
                                },
                                "sources": [{
                                    "fieldName": "authoredBooks",
                                    "filter": { "title": { "_eq": "A" } },
                                }],
                            },
                        },
                    }],
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
