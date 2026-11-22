//! Exact float equality against zero, on indexed and unindexed fields.
//! Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

type Action = ActionInstance<integration_test::actions::Actions>;

fn add_doc(name: &'static str, value: serde_json::Value) -> Action {
    AddDoc {
        collection: "F".into(),
        doc_map: Some(json!({ "name": name, "l": value, "lu": value })),
        ..Default::default()
    }
    .into()
}

fn users(filter: &str, names: &[&str]) -> Action {
    Request {
        request: format!(
            r#"
                query {{
                    F(filter: {filter}) {{
                        name
                    }}
                }}
            "#
        )
        .into(),
        expected_data: json!({
            "F": names.iter().map(|name| json!({ "name": name })).collect::<Vec<_>>(),
        }),
        common: Common {
            unordered_documents: vec!["/data/F".into()],
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("go.query.simple.filter.equality.eq")]
fn float_zero_equality_is_exact_on_indexed_and_unindexed_fields() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"type F { name: String l: Float @index lu: Float }"#.into(),
                ..Default::default()
            }
            .into(),
            add_doc("zero", json!(0.0)),
            add_doc("tiny", json!(1e-300)),
            add_doc("small", json!(2.7755575615628914e-17)),
            add_doc("one", json!(1.0)),
            users(r#"{lu: {_eq: 0.0}}"#, &["zero"]),
            users(r#"{l: {_eq: 0.0}}"#, &["zero"]),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.filter.equality.neq")]
fn float_zero_inequality_is_exact_on_indexed_and_unindexed_fields() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"type F { name: String l: Float @index lu: Float }"#.into(),
                ..Default::default()
            }
            .into(),
            add_doc("zero", json!(0.0)),
            add_doc("tiny", json!(1e-300)),
            add_doc("small", json!(2.7755575615628914e-17)),
            add_doc("one", json!(1.0)),
            users(r#"{_not: {lu: {_eq: 0.0}}}"#, &["tiny", "small", "one"]),
            users(r#"{_not: {l: {_eq: 0.0}}}"#, &["tiny", "small", "one"]),
        ],
        ..Default::default()
    }
    .execute();
}
