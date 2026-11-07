//! Introspection shows the aggregate arguments the executor accepts, as Go
//! does (#2649, `defra.request.aggregate_introspection`). Arguments are
//! checked as a subset: the test also runs against Go, which has more.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn arg(name: &str, ty: &str) -> Value {
    json!({ "name": name, "type": { "name": ty } })
}

#[test]
#[mantra_macros::req_test("defra.request.aggregate_introspection")]
fn aggregates_show_relation_and_root_arguments() {
    let numeric = || {
        json!([
            arg("GROUP", "Author__NumericSelector"),
            arg("published", "Book__NumericSelector"),
        ])
    };
    let root_numeric = |name: &str| {
        json!({
            "name": name,
            "type": { "kind": "SCALAR", "name": "Float" },
            "args": m::unordered(json!([
                arg("Author", "Author__NumericSelector"),
                arg("Book", "Book__NumericSelector"),
            ])),
        })
    };
    TestCase::from_actions(vec![
        AddCollection {
            sdl: r#"
                type Book { name: String rating: Float author: Author }
                type Author { name: String age: Int published: [Book] }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query {
                author: __type(name: "Author") { fields { name args { name type { name } } } }
                root: __type(name: "Query") { fields { name type { kind name } args { name type { name } } } }
            }"#
            .into(),
            expected_data: json!({
                "author": { "fields": m::all(json!([
                    m::first_where("name", "COUNT", json!({ "name": "COUNT", "args": m::contains(json!([
                        arg("GROUP", "Author__CountSelector"),
                        arg("_version", "Author___version__CountSelector"),
                        arg("published", "Book__CountSelector"),
                    ])) })),
                    m::first_where("name", "SUM", json!({ "name": "SUM", "args": m::contains(numeric()) })),
                    m::first_where("name", "AVG", json!({ "name": "AVG", "args": m::contains(numeric()) })),
                    m::first_where("name", "MIN", json!({ "name": "MIN", "args": m::contains(numeric()) })),
                    m::first_where("name", "MAX", json!({ "name": "MAX", "args": m::contains(numeric()) })),
                ])) },
                "root": { "fields": m::all(json!([
                    m::first_where("name", "MIN", root_numeric("MIN")),
                    m::first_where("name", "MAX", root_numeric("MAX")),
                ])) },
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
