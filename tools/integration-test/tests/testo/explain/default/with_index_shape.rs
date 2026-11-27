//! Pins the plan explain shape for an index-backed scalar filter.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn indexed_scan_explains_the_selected_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain {
                        User(filter: {age: {_eq: 30}}) {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "operationNode": [{
                            "selectTopNode": {
                                "selectNode": {
                                    "docID": null,
                                    "filter": null,
                                    "scanNode": {
                                        "collectionID": "bafyreiek4irx5zh2it7sokxwbqud2uanuhkqppnmt2dgan7r4gyafly6wi",
                                        "collectionName": "User",
                                        "filter": {"age": {"_eq": 30}},
                                        "indexLimit": null,
                                        "indexName": "User_age_ASC",
                                        "indexOffset": 0,
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn go_indexed_scan_explains_the_selected_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::GoHttp]),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain {
                        User(filter: {age: {_eq: 30}}) {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "operationNode": [{
                            "selectTopNode": {
                                "selectNode": {
                                    "docID": null,
                                    "filter": null,
                                    "scanNode": {
                                        "collectionID": "bafyreiek4irx5zh2it7sokxwbqud2uanuhkqppnmt2dgan7r4gyafly6wi",
                                        "collectionName": "User",
                                        "filter": {"age": {"_eq": 30}},
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
        ],
        ..Default::default()
    }
    .execute();
}

fn simple_indexed_scan(
    id: &'static str,
    client_types: &'static [ClientType],
    scan_node: serde_json::Value,
) {
    TestCase {
        id,
        client_types: Some(client_types),
        actions: vec![
            AddCollection {
                sdl: "type Book { title: String @index }".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: simple) {
                        Book(filter: {title: {_eq: "1984"}}) {
                            title
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "operationNode": [{
                            "selectTopNode": {
                                "selectNode": {
                                    "docID": null,
                                    "filter": null,
                                    "scanNode": scan_node,
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

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn simple_indexed_scan_reports_no_index_fetches() {
    simple_indexed_scan(
        integration_test::scenario_id!(),
        &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli],
        json!({
            "collectionID": "bafyreignnfga3vinwqt2awokqzfmovoggde2bug3q4xcq4ys6yxhwbzn3m",
            "collectionName": "Book",
            "filter": {"title": {"_eq": "1984"}},
            "indexLimit": null,
            "indexName": "Book_title_ASC",
            "indexOffset": 0,
            "prefixes": ["/1"],
        }),
    );
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn go_simple_indexed_scan_reports_no_index_fetches() {
    simple_indexed_scan(
        integration_test::scenario_id!(),
        &[ClientType::GoHttp],
        json!({
            "collectionID": "bafyreignnfga3vinwqt2awokqzfmovoggde2bug3q4xcq4ys6yxhwbzn3m",
            "collectionName": "Book",
            "filter": {"title": {"_eq": "1984"}},
            "prefixes": ["/1"],
        }),
    );
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn explain_relation_filter_uses_the_indexed_child_scan() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        actions: vec![
        AddCollection {
            sdl: "type Book { title: String author: Author } type Author { name: String @index books: [Book] }".into(),
            ..Default::default()
        }.into(),
        GetCollections {
            collection_name: Some("Author".into()),
            filtered: true,
            common: Common {
                capture: captures([("author_version", "/collections/0/VersionID")]),
                ..Default::default()
            },
            ..Default::default()
        }.into(),
        Request {
            request: r#"query @explain { Book(filter: {author: {name: {_like: "%vid%"}}}) { title } }"#.into(),
            expected_data: m::explain_target("scanNode", true, 1, json!({
                "collectionName": "Author",
                "collectionID": m::r("author_version"),
                "indexName": "Author_name_ASC",
                "indexLimit": null,
                "indexOffset": 0,
                "filter": {"name": {"_like": "%vid%"}},
                "prefixes": ["/2"]
            })),
            ..Default::default()
        }.into(),
        ],
        ..Default::default()
    }.execute();
}
