use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

fn user_with_descending_json_index() -> AddCollection {
    AddCollection {
        sdl: r#"
            type User {
                name: String
                custom: JSON @index(direction: DESC)
            }
        "#
        .into(),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.index.descending_json_path")]
fn descending_json_index_filters_an_array_path() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            user_with_descending_json_index().into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "custom": { "a": [1, 2] } })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User(filter: {custom: {a: {_any: {_gt: 1}}}}) {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.descending_json_path")]
fn descending_json_index_filters_a_nested_path_range() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            user_with_descending_json_index().into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "custom": { "height": 168 } })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "custom": { "height": 180 } })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User(filter: {custom: {height: {_gt: 170}}}) {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
