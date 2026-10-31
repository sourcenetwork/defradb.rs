//! Generated from the conformance scenarios of tests/integration/query/simple/with_similarity_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.full-scan")]
fn query_simple_with_float32_similarity_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Float32!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2, 4, 1] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User{
                                            name
                                            SIMILARITY(pointsList: {vector: [1, 2, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "SIMILARITY": m::similarity_score("COSINE", json!([2, 4, 1]), json!([1, 2, 0])),
                            "name": "John",
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
#[mantra_macros::req_test("go.query.simple.similarity.full-scan")]
fn query_simple_with_float64_similarity_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Float64!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2, 4, 1] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User{
                                            name
                                            SIMILARITY(pointsList: {vector: [1, 2, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "SIMILARITY": m::similarity_score("COSINE", json!([2, 4, 1]), json!([1, 2, 0])),
                            "name": "John",
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
#[mantra_macros::req_test("go.query.simple.similarity.full-scan")]
fn query_simple_with_int_similarity_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Int!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2, 4, 1] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User{
                                            name
                                            SIMILARITY(pointsList: {vector: [1, 2, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "SIMILARITY": m::similarity_score("COSINE", json!([2, 4, 1]), json!([1, 2, 0])),
                            "name": "John",
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
#[mantra_macros::req_test("go.query.simple.similarity.full-scan")]
fn query_simple_with_json_doc_creation_similarity_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Float64!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2, 4, 1] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User{
                                            name
                                            SIMILARITY(pointsList: {vector: [1, 2, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "SIMILARITY": m::similarity_score("COSINE", json!([2, 4, 1]), json!([1, 2, 0])),
                            "name": "John",
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
#[mantra_macros::req_test("go.query.simple.similarity.alias")]
fn query_simple_with_similarity_and_filtering_on_similarity_result_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Int!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2, 4, 1] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "pointsList": [1, 1, 1] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "pointsList": [4, 5, 3] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {_alias: {sim: {_lt: 0.9}}}){
                                            name
                                            sim: SIMILARITY(pointsList: {vector: [1, 2, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "Bob",
                            "sim": m::similarity_score("COSINE", json!([1, 1, 1]), json!([1, 2, 0])),
                        },
                        {
                            "name": "Alice",
                            "sim": m::similarity_score("COSINE", json!([4, 5, 3]), json!([1, 2, 0])),
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
#[mantra_macros::req_test("go.query.simple.similarity.alias")]
fn query_simple_with_similarity_and_ordering_with_limit_on_similarity_result_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Int!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2, 4, 1] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "pointsList": [1, 1, 1] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "pointsList": [4, 5, 3] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: DESC}}, limit: 2){
                                            name
                                            sim: SIMILARITY(pointsList: {vector: [1, 2, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "John",
                            "sim": m::similarity_score("COSINE", json!([2, 4, 1]), json!([1, 2, 0])),
                        },
                        {
                            "name": "Alice",
                            "sim": m::similarity_score("COSINE", json!([4, 5, 3]), json!([1, 2, 0])),
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
#[mantra_macros::req_test("go.query.simple.similarity.alias")]
fn query_simple_with_two_similarity_and_filtering_on_second_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Int!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2, 4, 1] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "pointsList": [1, 1, 1] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "pointsList": [4, 5, 3] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {_alias: {sim2: {_gt: 0.95}}}){
                                            name
                                            sim: SIMILARITY(pointsList: {vector: [1, 2, 0]})
                                            sim2: SIMILARITY(pointsList: {vector: [2, 3, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "John",
                            "sim": m::similarity_score("COSINE", json!([2, 4, 1]), json!([1, 2, 0])),
                            "sim2": m::similarity_score("COSINE", json!([2, 4, 1]), json!([2, 3, 0])),
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
#[mantra_macros::req_test("go.query.simple.similarity.validation")]
fn query_simple_with_int_similarity_different_vector_length_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Int!]
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2, 4, 1] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User{
                                            name
                                            SIMILARITY(pointsList: {vector: [1, 2, 0, 1]})
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    "source and vector must be of the same length. Source: 3, Vector: 4".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.validation")]
fn query_simple_with_similarity_and_no_arguments_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Int!]
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            SIMILARITY
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("similarity requires a target field argument".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.non-numeric-target")]
fn query_simple_with_similarity_and_wrong_field_type_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pets: [String!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User{
                                            SIMILARITY(pets: {vector: [1.1, 1.2, 0.9]})
                                        }
                                    }
                "#.into(),
                expected_error: Some("similarity can only target a numeric array field. Field: pets, Type: [String!]".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.validation")]
fn query_simple_with_similarity_and_wrong_vector_value_type_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Int!]
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User{
                                            SIMILARITY(pointsList: {vector: [1.1, 1.2, 0.9]})
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Argument "pointsList" has invalid value {vector: [1.1, 1.2, 0.9]}.
In field "vector": In element #1: Expected type "Int", found 1.1.
In field "vector": In element #1: Expected type "Int", found 1.2.
In field "vector": In element #1: Expected type "Int", found 0.9."#
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.non-numeric-target")]
fn query_simple_with_similarity_in_fragment_and_wrong_field_type_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        books: [Book]
                                    }
                                    type Book {
                                        name: String
                                        tags: [String!]
                                        author: User
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            books {
                                                ...similarity
                                            }
                                        }
                                    }
                                    fragment similarity on Book {
                                        SIMILARITY(tags: {vector: [1.1, 1.2, 0.9]})
                                    }
                "#.into(),
                expected_error: Some("similarity can only target a numeric array field. Field: tags, Type: [String!]".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.non-numeric-target")]
fn query_simple_with_similarity_in_inline_fragment_and_wrong_field_type_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pets: [String!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            ... on User {
                                                SIMILARITY(pets: {vector: [1.1, 1.2, 0.9]})
                                            }
                                        }
                                    }
                "#.into(),
                expected_error: Some("similarity can only target a numeric array field. Field: pets, Type: [String!]".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.non-numeric-target")]
fn query_simple_with_similarity_in_mutation_and_wrong_field_type_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pets: [String!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {name: "John"}) {
                                            SIMILARITY(pets: {vector: [1.1, 1.2, 0.9]})
                                        }
                                    }
                "#.into(),
                expected_error: Some("similarity can only target a numeric array field. Field: pets, Type: [String!]".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.full-scan")]
fn query_simple_with_similarity_on_empty_collection_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Int!]
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User{
                                            SIMILARITY(pointsList: {vector: [1, 2, 3]})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.select.unknown-field")]
fn query_simple_with_similarity_on_query_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Int!]
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        SIMILARITY
                                    }
                "#
                .into(),
                expected_error: Some(r#"Cannot query field "SIMILARITY" on type "Query"."#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.validation")]
fn query_simple_with_similarity_on_undefined_field_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User{
                                            SIMILARITY(pointsList: {vector: [1, 2, 3]})
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Unknown argument "pointsList" on field "SIMILARITY" of type "User"."#.into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.alias")]
fn query_simple_with_two_similarity_and_filtering_on_both_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Int!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2, 4, 1] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "pointsList": [1, 1, 1] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "pointsList": [4, 5, 3] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {_or: [{_alias: {sim2: {_gt: 20}}}, {_alias: {sim: {_lt: 10}}}]}){
                                            name
                                            sim: SIMILARITY(pointsList: {vector: [1, 2, 0]})
                                            sim2: SIMILARITY(pointsList: {vector: [2, 3, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
