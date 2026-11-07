//! Introspection shows the arguments a relation field takes, as Go does
//! (#2649, `defra.request.relation_field_args`).

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

fn arg(name: &str, ty: serde_json::Value) -> serde_json::Value {
    json!({ "name": name, "type": ty })
}

fn named(kind: &str, name: &str) -> serde_json::Value {
    json!({ "kind": kind, "name": name, "ofType": null })
}

fn list(of: serde_json::Value) -> serde_json::Value {
    json!({ "kind": "LIST", "name": null, "ofType": of })
}

fn non_null(of: serde_json::Value) -> serde_json::Value {
    json!({ "kind": "NON_NULL", "name": null, "ofType": of })
}

#[test]
#[mantra_macros::req_test("defra.request.relation_field_args")]
fn relation_fields_show_their_arguments() {
    let fields = r#"fields { name args { name type { kind name ofType { kind name ofType { kind name ofType { kind name } } } } } }"#;
    TestCase::from_actions(vec![
        AddCollection {
            sdl: r#"
                type Book { name: String author: Author }
                type Author { name: String published: [Book] }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!(
                r#"query {{ book: __type(name: "Book") {{ {fields} }} author: __type(name: "Author") {{ {fields} }} }}"#
            )
            .into(),
            expected_data: json!({
                "book": { "fields": m::contains(json!([{
                    "name": "author",
                    "args": [arg("filter", named("INPUT_OBJECT", "AuthorFilterArg"))],
                }])) },
                "author": { "fields": m::contains(json!([{
                    "name": "published",
                    "args": [
                        arg("docID", list(non_null(named("SCALAR", "ID")))),
                        arg("filter", named("INPUT_OBJECT", "BookFilterArg")),
                        arg("groupBy", list(non_null(named("ENUM", "BookField")))),
                        arg("limit", named("SCALAR", "Int")),
                        arg("offset", named("SCALAR", "Int")),
                        arg("order", list(named("INPUT_OBJECT", "BookOrderArg"))),
                    ],
                }])) },
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
