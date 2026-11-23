//! Explain shows a read through an index as Go's plain scan node, with its
//! filter and prefixes.

use integration_test::actions::{AddCollection, GetCollections, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

const PERSON_BOOKS: &str = r#"
    type Person {
        name: String @index
        authoredBooks: [Book]
    }
    type Book {
        title: String
        pages: Int
        author: Person
    }
"#;

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

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn explain_shows_an_index_read_as_a_plain_scan_node() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: PERSON_BOOKS.into(),
            ..Default::default()
        }
        .into(),
        capture_version("Person", "person_version").into(),
        Request {
            request: r#"query @explain {
                Book(filter: {author: {name: {_like: "%vid%"}}}) { title }
            }"#
            .into(),
            expected_data: json!(m::explain_target(
                "scanNode",
                false,
                1,
                json!({
                    "collectionID": m::r("person_version"),
                    "collectionName": "Person",
                    "filter": { "name": { "_like": "%vid%" } },
                    "prefixes": ["/1"],
                }),
            )),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn explain_of_a_top_level_index_read_is_a_plain_scan_node() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type User { name: String  age: Int @index }".into(),
            ..Default::default()
        }
        .into(),
        capture_version("User", "user_version").into(),
        Request {
            request: r#"query @explain { User(filter: {age: {_eq: 30}}) { name } }"#.into(),
            expected_data: json!({
                "explain": {
                    "operationNode": [{
                        "selectTopNode": {
                            "selectNode": {
                                "docID": null,
                                "filter": null,
                                "scanNode": {
                                    "collectionID": m::r("user_version"),
                                    "collectionName": "User",
                                    "filter": { "age": { "_eq": 30 } },
                                    "prefixes": ["/1"],
                                },
                            },
                        },
                    }],
                },
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn a_join_from_the_indexed_child_names_the_parent_side() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: PERSON_BOOKS.into(),
            ..Default::default()
        }
        .into(),
        capture_version("Person", "person_version").into(),
        capture_version("Book", "book_version").into(),
        Request {
            request: r#"query @explain {
                Book(filter: {author: {name: {_eq: "David"}}}) { title author { name } }
            }"#
            .into(),
            expected_data: json!({
                "explain": {
                    "operationNode": [{
                        "selectTopNode": {
                            "selectNode": {
                                "docID": null,
                                "filter": { "author": { "name": { "_eq": "David" } } },
                                "typeIndexJoin": {
                                    "direction": "primary",
                                    "joinType": "typeJoinOne",
                                    "root": {
                                        "scanNode": {
                                            "collectionID": m::r("book_version"),
                                            "collectionName": "Book",
                                            "filter": null,
                                            "prefixes": ["/2"],
                                        },
                                    },
                                    "rootName": "authoredBooks",
                                    "subType": {
                                        "selectTopNode": {
                                            "selectNode": {
                                                "docID": null,
                                                "filter": null,
                                                "scanNode": {
                                                    "collectionID": m::r("person_version"),
                                                    "collectionName": "Person",
                                                    "filter": { "name": { "_eq": "David" } },
                                                    "prefixes": ["/1"],
                                                },
                                            },
                                        },
                                    },
                                    "subTypeName": "author",
                                },
                            },
                        },
                    }],
                },
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
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

/// Go's explain of `Person` joined to its books, the books read through
/// `book_scan`.
fn person_books_join(book_scan: serde_json::Value) -> serde_json::Value {
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
                        "filter": null,
                        "scanNode": book_scan,
                    },
                },
            },
            "subTypeName": "authoredBooks",
        },
    })
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn an_index_read_shows_its_aggregate_filter_too() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: PERSON_INDEXED_BOOKS.into(),
            ..Default::default()
        }
        .into(),
        capture_version("Person", "person_version").into(),
        capture_version("Book", "book_version").into(),
        Request {
            request: r#"query @explain {
                Person { name COUNT(authoredBooks: {filter: {title: {_eq: "A"}, pages: {_gt: 100}}}) }
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
                                    "typeIndexJoin": person_books_join(json!({
                                        "collectionID": m::r("book_version"),
                                        "collectionName": "Book",
                                        "filter": { "pages": { "_gt": 100 }, "title": { "_eq": "A" } },
                                        "prefixes": ["/2"],
                                    }))["typeIndexJoin"],
                                },
                                "sources": [{
                                    "fieldName": "authoredBooks",
                                    "filter": { "pages": { "_gt": 100 }, "title": { "_eq": "A" } },
                                }],
                            },
                        },
                    }],
                },
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn an_indexed_child_and_an_aggregate_filter_each_keep_their_filter() {
    TestCase::from_actions(vec![
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
                                        person_books_join(json!({
                                            "collectionID": m::r("book_version"),
                                            "collectionName": "Book",
                                            "filter": { "title": { "_eq": "A" } },
                                            "prefixes": ["/2"],
                                        })),
                                        person_books_join(json!({
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
                                    "typeIndexJoin": person_books_join(json!({
                                        "collectionID": m::r("book_version"),
                                        "collectionName": "Book",
                                        "filter": { "title": { "_eq": "A" } },
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
    ])
    .execute();
}
