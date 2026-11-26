use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

fn select_unknown_object_field(id: &'static str, request: &str, expected_error: &str) {
    TestCase {
        id,
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "1984" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: request.into(),
                expected_error: Some(expected_error.into()),
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
fn query_simple_with_commits_field_on_collection() {
    select_unknown_object_field(
        integration_test::scenario_id!(),
        r#"
            query {
                Book(filter: { title: { _eq: "1984" } }) {
                    title
                    _commits {
                        fieldName
                        cid
                        height
                    }
                }
            }
        "#,
        r#"Cannot query field "_commits" on type "Book"."#,
    );
}

#[test]
#[mantra_macros::req_test("go.query.simple.select.unknown-field")]
fn query_simple_with_undefined_object_field() {
    select_unknown_object_field(
        integration_test::scenario_id!(),
        r#"
            query {
                Book {
                    title
                    author {
                        name
                    }
                }
            }
        "#,
        r#"Cannot query field "author" on type "Book"."#,
    );
}
