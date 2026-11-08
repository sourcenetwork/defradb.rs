//! `BM25` fields can follow a relation (`file.content`, `functions.name`), and
//! a `BM25` score can be selected, filtered and ordered on a nested selection.
//! Two files, `auth.rs` and `utils.rs`, each own functions.

use super::simple::RUST_CLIENTS;
use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const SDL: &str = r#"
type File {
    name: String @fulltext
    path: String @fulltext
    content: String @fulltext
    functions: [Function]
}

type Function {
    name: String @fulltext
    content: String @fulltext
    qualifiedName: String
    startLine: Int
    file: File @primary
}
"#;

fn files() -> Vec<Actions> {
    let file = |name: &str, content: &str| -> Actions {
        AddDoc {
            collection: "File".into(),
            doc_map: Some(
                json!({ "name": name, "path": format!("src/{name}"), "content": content }),
            ),
            common: Common {
                capture: captures([(name, "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into()
    };
    vec![
        AddCollection {
            sdl: SDL.into(),
            ..Default::default()
        }
        .into(),
        file("auth.rs", "auth middleware hooks"),
        file("utils.rs", "shared helpers"),
    ]
}

fn function(
    file: &str,
    name: &str,
    qualified_name: &str,
    start_line: u64,
    content: &str,
) -> Actions {
    AddDoc {
        collection: "Function".into(),
        doc_map: Some(json!({
            "name": name,
            "qualifiedName": qualified_name,
            "startLine": start_line,
            "content": content,
            "_fileID": m::r(file),
        })),
        ..Default::default()
    }
    .into()
}

/// Each file has a `handle_request` that mentions nothing about auth.
fn handle_requests() -> Vec<Actions> {
    vec![
        function(
            "auth.rs",
            "handle_request",
            "auth::handle_request",
            10,
            "handles inbound requests",
        ),
        function(
            "utils.rs",
            "handle_request",
            "utils::handle_request",
            20,
            "handles inbound requests",
        ),
    ]
}

/// `auth.rs` has a function about parsing tokens, `utils.rs` one about output.
fn parse_token_and_format_output() -> Vec<Actions> {
    vec![
        function(
            "auth.rs",
            "parse token",
            "auth::parse_token",
            15,
            "parse token auth flow",
        ),
        function(
            "utils.rs",
            "format_output",
            "utils::format_output",
            30,
            "format shared output",
        ),
    ]
}

fn request(request: &str, expected: Value, unordered: &[&str]) -> Actions {
    Request {
        request: request.into(),
        expected_data: expected,
        common: Common {
            unordered_documents: unordered.iter().map(|path| (*path).into()).collect(),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn case(actions: Vec<Vec<Actions>>) {
    TestCase {
        actions: actions.into_iter().flatten().map(Into::into).collect(),
        client_types: Some(RUST_CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.bm25.relation_fields")]
fn a_parent_field_scores_its_children() {
    let query = |condition: &str| {
        format!(
            r#"query {{
                Function(filter: {{_alias: {{score: {condition}}}}}) {{
                    qualifiedName
                    score: BM25(query: "auth", fields: ["name", "content", "file.content"])
                }}
            }}"#
        )
    };
    case(vec![
        files(),
        handle_requests(),
        vec![
            request(
                &query("{_gt: 0}"),
                json!({ "Function": [m::object(json!({ "qualifiedName": "auth::handle_request" }))] }),
                &[],
            ),
            request(
                &query("{_eq: 0}"),
                json!({ "Function": [m::object(json!({ "qualifiedName": "utils::handle_request" }))] }),
                &[],
            ),
        ],
    ]);
}

#[test]
#[mantra_macros::req_test("defra.bm25.relation_fields")]
fn child_fields_score_their_parent_and_order_it() {
    case(vec![
        files(),
        parse_token_and_format_output(),
        vec![request(
            r#"query {
                File(order: {_alias: {score: DESC}}) {
                    name
                    score: BM25(query: "parse token", fields: ["name", "functions.name", "functions.content"])
                }
            }"#,
            json!({ "File": [
                m::object(json!({ "name": "auth.rs" })),
                { "name": "utils.rs", "score": 0.0 },
            ] }),
            &[],
        )],
    ]);
}

#[test]
#[mantra_macros::req_test("defra.bm25.nested_selection")]
fn a_nested_one_to_many_selection_scores_each_child() {
    case(vec![
        files(),
        handle_requests(),
        vec![request(
            r#"query {
                File(order: {name: ASC}) {
                    name
                    functions(filter: {_alias: {score: {_gt: 0}}}) {
                        qualifiedName
                        score: BM25(query: "auth", fields: ["name", "content", "file.content"])
                    }
                }
            }"#,
            json!({ "File": [
                { "name": "auth.rs", "functions": [m::object(json!({ "qualifiedName": "auth::handle_request" }))] },
                { "name": "utils.rs", "functions": [] },
            ] }),
            &[],
        )],
    ]);
}

#[test]
#[mantra_macros::req_test("defra.bm25.nested_selection")]
fn a_nested_one_to_one_selection_scores_the_parent() {
    case(vec![
        files(),
        parse_token_and_format_output(),
        vec![request(
            r#"query {
                Function(order: {qualifiedName: ASC}) {
                    qualifiedName
                    file {
                        name
                        score: BM25(query: "parse token", fields: ["name", "functions.name", "functions.content"])
                    }
                }
            }"#,
            json!({ "Function": [
                { "qualifiedName": "auth::parse_token", "file": m::object(json!({ "name": "auth.rs" })) },
                { "qualifiedName": "utils::format_output", "file": { "name": "utils.rs", "score": 0.0 } },
            ] }),
            &[],
        )],
    ]);
}

#[test]
#[mantra_macros::req_test("defra.bm25.nested_selection")]
fn a_nested_selection_filters_and_orders_by_its_score() {
    case(vec![
        files(),
        vec![function(
            "auth.rs",
            "auth_guard",
            "auth::auth_guard",
            5,
            "auth guard middleware",
        )],
        handle_requests(),
        vec![request(
            r#"query {
                File(order: {name: ASC}) {
                    name
                    functions(filter: {_alias: {score: {_gt: 0}}}, order: {_alias: {score: DESC}}) {
                        qualifiedName
                        score: BM25(query: "auth guard", fields: ["name", "content", "file.content"])
                    }
                }
            }"#,
            json!({ "File": [
                { "name": "auth.rs", "functions": [
                    m::object(json!({ "qualifiedName": "auth::auth_guard" })),
                    m::object(json!({ "qualifiedName": "auth::handle_request" })),
                ] },
                { "name": "utils.rs", "functions": [] },
            ] }),
            &[],
        )],
    ]);
}
