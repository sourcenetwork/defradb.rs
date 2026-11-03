//! Generated from the conformance scenarios of tests/integration/explain/default/group_with_average_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.group.average")]
fn default_explain_request_with_average_inside_the_inner_group_and_nested_group_by_with_average() {
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
                                        Author (groupBy: [name]) {
                                            name
                                            AVG(GROUP: {field: AVG})
                                            GROUP(groupBy: [verified]) {
                                                verified
                                                    AVG(GROUP: {field: age})
                                                    GROUP (groupBy: [age]){
                                                        age
                                                        AVG(GROUP: {field: age})
                                                    }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "averageNode": {
                                            "countNode": {
                                                "sumNode": {
                                                    "groupNode": { "selectNode": { "scanNode": {} } },
                                                },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("groupNode", false, 0, json!({
                        "childSelects": [
                            {
                                "collectionName": "Author",
                                "docID": null,
                                "filter": null,
                                "groupBy": ["verified", "name"],
                                "limit": null,
                                "orderBy": null,
                            },
                        ],
                        "groupByFields": ["name"],
                    })),
                    m::explain_target("averageNode", false, 0, json!({})),
                    m::explain_target("countNode", false, 0, json!({ "sources": [{ "fieldName": "GROUP", "filter": null }] })),
                    m::explain_target("sumNode", false, 0, json!({
                        "sources": [
                            {
                                "childFieldName": "AVG",
                                "fieldName": "GROUP",
                                "filter": null,
                            },
                        ],
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

#[test]
#[mantra_macros::req_test("go.explain.default.group.average")]
fn default_explain_request_with_average_inside_the_inner_group_on_a_field_and_nested_group_by() {
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
                                        Author (groupBy: [name]) {
                                            name
                                            AVG(GROUP: {field: AVG})
                                            GROUP(groupBy: [verified]) {
                                                verified
                                                    AVG(GROUP: {field: age})
                                                    GROUP (groupBy: [age]){
                                                        age
                                                    }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "averageNode": {
                                            "countNode": {
                                                "sumNode": {
                                                    "groupNode": { "selectNode": { "scanNode": {} } },
                                                },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("groupNode", false, 0, json!({
                        "childSelects": [
                            {
                                "collectionName": "Author",
                                "docID": null,
                                "filter": null,
                                "groupBy": ["verified", "name"],
                                "limit": null,
                                "orderBy": null,
                            },
                        ],
                        "groupByFields": ["name"],
                    })),
                    m::explain_target("averageNode", false, 0, json!({})),
                    m::explain_target("countNode", false, 0, json!({ "sources": [{ "fieldName": "GROUP", "filter": null }] })),
                    m::explain_target("sumNode", false, 0, json!({
                        "sources": [
                            {
                                "childFieldName": "AVG",
                                "fieldName": "GROUP",
                                "filter": null,
                            },
                        ],
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

#[test]
#[mantra_macros::req_test("go.explain.default.group.average")]
fn default_explain_request_with_average_inside_the_inner_group_on_a_field() {
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
                                        Author (groupBy: [name]) {
                                            name
                                            AVG(GROUP: {field: AVG})
                                            GROUP(groupBy: [verified]) {
                                                verified
                                                AVG(GROUP: {field: age})
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "averageNode": {
                                            "countNode": {
                                                "sumNode": {
                                                    "groupNode": { "selectNode": { "scanNode": {} } },
                                                },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("groupNode", false, 0, json!({
                        "childSelects": [
                            {
                                "collectionName": "Author",
                                "docID": null,
                                "filter": null,
                                "groupBy": ["verified", "name"],
                                "limit": null,
                                "orderBy": null,
                            },
                        ],
                        "groupByFields": ["name"],
                    })),
                    m::explain_target("averageNode", false, 0, json!({})),
                    m::explain_target("countNode", false, 0, json!({ "sources": [{ "fieldName": "GROUP", "filter": null }] })),
                    m::explain_target("sumNode", false, 0, json!({
                        "sources": [
                            {
                                "childFieldName": "AVG",
                                "fieldName": "GROUP",
                                "filter": null,
                            },
                        ],
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

#[test]
#[mantra_macros::req_test("go.explain.default.group.average")]
fn default_explain_request_with_group_by_with_average_on_an_inner_field() {
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
                                        Author (groupBy: [name]) {
                                            name
                                            AVG(GROUP: {field: age})
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "averageNode": {
                                            "countNode": {
                                                "sumNode": {
                                                    "groupNode": { "selectNode": { "scanNode": {} } },
                                                },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("groupNode", false, 0, json!({
                        "childSelects": [
                            {
                                "collectionName": "Author",
                                "docID": null,
                                "filter": { "age": { "_neq": null } },
                                "groupBy": null,
                                "limit": null,
                                "orderBy": null,
                            },
                        ],
                        "groupByFields": ["name"],
                    })),
                    m::explain_target("averageNode", false, 0, json!({})),
                    m::explain_target("countNode", false, 0, json!({
                        "sources": [
                            {
                                "fieldName": "GROUP",
                                "filter": { "age": { "_neq": null } },
                            },
                        ],
                    })),
                    m::explain_target("sumNode", false, 0, json!({
                        "sources": [
                            {
                                "childFieldName": "age",
                                "fieldName": "GROUP",
                                "filter": { "age": { "_neq": null } },
                            },
                        ],
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
