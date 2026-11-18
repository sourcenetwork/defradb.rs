//! Introspection shows Go's mutation surface: the arguments and types the
//! mutation parser accepts (#2649, `defra.request.mutation_introspection`).
//! Arguments are checked as a subset, since the test also runs against Go,
//! which has no `docIDs`.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn named(kind: &str, name: &str) -> Value {
    json!({ "kind": kind, "name": name, "ofType": null })
}

fn list(of: Value) -> Value {
    json!({ "kind": "LIST", "name": null, "ofType": of })
}

fn non_null(of: Value) -> Value {
    json!({ "kind": "NON_NULL", "name": null, "ofType": of })
}

fn field(name: &str, ty: Value, args: &[(&str, Value)]) -> Value {
    let args: Vec<Value> = args
        .iter()
        .map(|(name, ty)| json!({ "name": name, "type": ty }))
        .collect();
    json!({ "name": name, "type": ty, "args": m::contains(json!(args)) })
}

#[test]
#[mantra_macros::req_test("defra.request.mutation_introspection")]
fn mutations_show_go_arguments_and_types() {
    let ty = "type { kind name ofType { kind name ofType { kind name ofType { kind name } } } }";
    let book = || named("OBJECT", "Book");
    let input = || named("INPUT_OBJECT", "BookMutationInputArg");
    let filter = || named("INPUT_OBJECT", "BookFilterArg");
    let ids = || list(non_null(named("SCALAR", "ID")));
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Book { name: String rating: Float }".into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!(
                r#"query {{
                    mutation: __type(name: "Mutation") {{ fields {{ name {ty} args {{ name {ty} }} }} }}
                    explicit: __type(name: "BookExplicitField") {{ enumValues {{ name }} }}
                }}"#
            )
            .into(),
            expected_data: json!({
                "mutation": { "fields": m::all(json!([
                    m::first_where("name", "add_Book", field("add_Book", list(book()), &[
                        ("encrypt", named("SCALAR", "Boolean")),
                        ("encryptFields", list(non_null(named("ENUM", "BookExplicitField")))),
                        ("input", list(non_null(input()))),
                    ])),
                    m::first_where("name", "update_Book", field("update_Book", list(book()), &[
                        ("docID", ids()),
                        ("filter", filter()),
                        ("input", input()),
                    ])),
                    m::first_where("name", "upsert_Book", field("upsert_Book", list(book()), &[
                        ("add", non_null(input())),
                        ("filter", non_null(filter())),
                        ("update", non_null(input())),
                    ])),
                    m::first_where("name", "delete_Book", field("delete_Book", list(book()), &[
                        ("docID", ids()),
                        ("filter", filter()),
                    ])),
                ])) },
                "explicit": { "enumValues": m::unordered(json!([{ "name": "name" }, { "name": "rating" }])) },
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

fn boolean_placeholder() -> Value {
    json!({ "name": "_", "type": named("SCALAR", "Boolean") })
}

/// A GraphQL object type needs at least one field, so Mutation always starts
/// with Go's `_` placeholder, even on a node with no collections (#3062).
#[test]
#[mantra_macros::req_test("defra.request.mutation_introspection")]
fn mutation_type_has_placeholder_without_collections() {
    TestCase::from_actions(vec![Request {
        request: r#"query {
                __schema { mutationType { name } }
                mutation: __type(name: "Mutation") { fields { name type { kind name ofType { kind name } } } }
            }"#
        .into(),
        expected_data: json!({
            "__schema": { "mutationType": { "name": "Mutation" } },
            "mutation": { "fields": [boolean_placeholder()] },
        }),
        ..Default::default()
    }
    .into()])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.request.mutation_introspection")]
fn mutation_type_keeps_placeholder_with_collections() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Book { name: String }".into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query {
                    mutation: __type(name: "Mutation") { fields { name type { kind name ofType { kind name } } } }
                }"#
            .into(),
            expected_data: json!({
                "mutation": { "fields": m::all(json!([
                    m::first_where("name", "_", boolean_placeholder()),
                    m::first_where("name", "add_Book", m::nonempty()),
                ])) },
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
