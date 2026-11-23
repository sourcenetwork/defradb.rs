//! A filter naming a field or operator its input type does not define fails
//! validation with graphql-go's text, whether the request wrote it or a
//! variable supplied it. Every text was measured on Go DefraDB 307364d9d.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const SDL: &str = r#"
    type Author { name: String age: Int books: [Book] }
    type Book { title: String rating: Float author: Author }
"#;

/// The clients a Rust node serves. Go's blocks lack the filter extensions
/// some of these tests use, so a Go node refuses them.
const RUST_CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

fn rejects(id: &'static str, request: &str, variables: Option<Value>, error: &str) {
    TestCase {
        id,
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "A", "age": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: request.into(),
                variables,
                expected_error: Some(error.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_literal_filter_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query { Author(filter: {nope: {_eq: 1}}) { name } }",
        None,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_operator_in_a_literal_filter_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        r#"query { Author(filter: {name: {_eqq: "a"}}) { name } }"#,
        None,
        "Argument \"filter\" has invalid value {name: {_eqq: \"a\"}}.\nIn field \"name\": In field \"_eqq\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_under_a_relation_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query { Author(filter: {books: {nope: {_eq: 1}}}) { name } }",
        None,
        "In field \"books\": In field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_an_and_element_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query { Author(filter: {_and: [{nope: {_eq: 1}}]}) { name } }",
        None,
        "In field \"_and\": In element #1: In field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_relation_selection_filter_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query { Author { name books(filter: {nope: {_eq: 1}}) { title } } }",
        None,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_commits_filter_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query { _commits(filter: {nope: {_eq: 1}}) { cid } }",
        None,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_filter_variable_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query($f: AuthorFilterArg) { Author(filter: $f) { name } }",
        Some(json!({ "f": { "nope": { "_eq": 1 } } })),
        "Variable \"$f\" got invalid value {\"nope\":{\"_eq\":1}}.\nIn field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_list_filter_variable_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query($f: AuthorFilterArg) { Author(filter: $f) { name } }",
        Some(json!({ "f": [{ "nope": { "_eq": 1 } }] })),
        "Variable \"$f\" got invalid value [{\"nope\":{\"_eq\":1}}].\nExpected \"AuthorFilterArg\", found not an object.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_non_object_relation_variable_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query($a: AuthorFilterArg) { Book(filter: {author: $a}) { title } }",
        Some(json!({ "a": 1 })),
        "Variable \"$a\" got invalid value 1.\nExpected \"AuthorFilterArg\", found not an object.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_operator_in_a_nested_variable_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query($n: StringOperatorBlock) { Author(filter: {name: $n}) { name } }",
        Some(json!({ "n": { "_eqq": "A" } })),
        "Variable \"$n\" got invalid value {\"_eqq\":\"A\"}.\nIn field \"_eqq\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_bad_literal_is_reported_before_its_variables() {
    rejects(
        integration_test::scenario_id!(),
        "query($n: String) { Author(filter: {name: {_eq: $n}, nope: {_eq: 1}}) { name } }",
        Some(json!({ "n": "A" })),
        "Argument \"filter\" has invalid value {name: {_eq: $n}, nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn filters_the_input_type_defines_still_run() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "A", "age": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query($n: StringOperatorBlock) {
                    Author(filter: {
                        name: $n,
                        _and: [{age: {_gt: 1}}, {_not: {age: {_lt: 2}}}],
                        _or: [{books: {rating: {_gt: 1}}}, {age: {_in: [30]}}]
                    }) { name }
                }"#
                .into(),
                variables: Some(json!({ "n": { "_eq": "A" } })),
                expected_data: json!({ "Author": [{ "name": "A" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_whole_and_list_variable_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query($c: [AuthorFilterArg!]) { Author(filter: {_and: $c}) { name } }",
        Some(json!({ "c": [{ "nope": { "_eq": 1 } }] })),
        "Variable \"$c\" got invalid value [{\"nope\":{\"_eq\":1}}].\nIn element #1: In field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_commits_links_filter_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query { _commits { cid links(filter: {nope: {_eq: 1}}) { cid } } }",
        None,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_commits_heads_filter_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query { _commits { cid heads(filter: {nope: {_eq: 1}}) { cid } } }",
        None,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_version_links_filter_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query { Author { name _version { cid links(filter: {nope: {_eq: 1}}) { cid } } } }",
        None,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_group_filter_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        "query { Author(groupBy: [name]) { name GROUP(filter: {nope: {_eq: 1}}) { age } } }",
        None,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

/// Range operators on text are a Rust extension Go's `StringOperatorBlock`
/// lacks: text compares lexicographically, so they keep filtering.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn range_operators_on_a_string_field_still_filter() {
    let author = |name: &str| AddDoc {
        collection: "Author".into(),
        doc_map: Some(json!({ "name": name })),
        ..Default::default()
    };
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            author("Alice").into(),
            author("Bob").into(),
            author("Carol").into(),
            Request {
                request: r#"query {
                    Author(filter: {name: {_gt: "B", _le: "Bz"}}) { name }
                }"#
                .into(),
                expected_data: json!({ "Author": [{ "name": "Bob" }] }),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(RUST_CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_literal_error_in_a_later_root_wins_over_a_variable_error() {
    rejects(
        integration_test::scenario_id!(),
        "query($f: AuthorFilterArg) { Author(filter: $f) { name } Book(filter: {nope: {_eq: 1}}) { title } }",
        Some(json!({ "f": { "gone": { "_eq": 1 } } })),
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

/// `_ne`, a Rust alias of `_neq`, is a list block's operator as it is a
/// scalar block's.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn the_ne_alias_filters_a_list_field() {
    let user = |tags: Value| AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "tags": tags })),
        ..Default::default()
    };
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { tags: [String] }".into(),
                ..Default::default()
            }
            .into(),
            user(json!(["a"])).into(),
            user(json!(["b"])).into(),
            Request {
                request: r#"query { User(filter: {tags: {_ne: ["a"]}}) { tags } }"#.into(),
                expected_data: json!({ "User": [{ "tags": ["b"] }] }),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(RUST_CLIENTS),
        ..Default::default()
    }
    .execute();
}
