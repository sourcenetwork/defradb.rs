//! Migrations over a collection with a unique index, among them ones that give
//! two documents the same value.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    Actions, AddCollection, AddDoc, GetCollections, ListIndexes, PatchCollection, Request,
    SetActiveCollectionVersion,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};
use testo::action::ActionInstance;

fn users_with_unique_age() -> Vec<ActionInstance<Actions>> {
    vec![
        AddCollection {
            sdl: r#"
                type Users {
                    name: String
                    age: Int @index(unique: true)
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        GetCollections {
            filtered: true,
            common: Common {
                capture: captures([("version1", "/collections/0/VersionID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "age": 20, "name": "Andy" })),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "age": 30, "name": "John" })),
            ..Default::default()
        }
        .into(),
    ]
}

fn patch_with_lens(arguments: Value, path: &str) -> ActionInstance<Actions> {
    PatchCollection {
        patch: r#"
            [
                { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "level", "Kind": "Int"} }
            ]
        "#
        .into(),
        lens: Some(Lens {
            lenses: Some(vec![LensModule {
                arguments,
                path: path.into(),
            }]),
        }),
        ..Default::default()
    }
    .into()
}

/// The listing of `Users_age_ASC`.
fn age_index_listed(execution: Value) -> ActionInstance<Actions> {
    ListIndexes {
        collection: "Users".into(),
        expected_data: json!({
            "indexes": [m::object(json!({
                "Description": m::object(json!({ "Name": "Users_age_ASC", "Unique": true })),
                "Execution": m::object(execution),
            }))],
        }),
        ..Default::default()
    }
    .into()
}

/// The listing of `Users_age_ASC` as a build the `age: 5` clash stopped.
fn age_index_failed() -> ActionInstance<Actions> {
    age_index_listed(json!({
        "Action": 3,
        "Status": 2,
        "Reason": m::all(json!([
            m::substring("can not index a doc's field(s) that violates unique index. DocID: bae-"),
            m::substring(", age: 5"),
        ])),
    }))
}

fn query_age(age: i64, expected: Value) -> ActionInstance<Actions> {
    Request {
        request: format!(
            r#"
                query {{
                    Users(filter: {{age: {{_eq: {age}}}}}) {{
                        name
                        age
                    }}
                }}
            "#
        )
        .into(),
        expected_data: json!({ "Users": m::unordered(expected) }),
        ..Default::default()
    }
    .into()
}

fn explain_age(age: i64, index_fetches: i64) -> ActionInstance<Actions> {
    Request {
        request: format!(
            r#"
                query @explain(type: execute) {{
                    Users(filter: {{age: {{_eq: {age}}}}}) {{
                        name
                    }}
                }}
            "#
        )
        .into(),
        expected_data: json!(m::explain_scan(json!({ "indexFetches": index_fetches }))),
        ..Default::default()
    }
    .into()
}

fn run(id: &'static str, steps: Vec<ActionInstance<Actions>>) {
    let mut actions = users_with_unique_age();
    actions.extend(steps);
    TestCase {
        id,
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.query.index.unique_clash")]
fn migration_giving_two_documents_one_unique_value_fails_the_index() {
    run(
        integration_test::scenario_id!(),
        vec![
            patch_with_lens(
                json!({ "dst": "age", "value": 5 }),
                "bin/lenses/set-default.wasm",
            ),
            age_index_failed(),
            query_age(
                5,
                json!([{ "age": 5, "name": "Andy" }, { "age": 5, "name": "John" }]),
            ),
            explain_age(5, 0),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 5, "name": "Fred" })),
                ..Default::default()
            }
            .into(),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.query.index.unique_clash")]
fn switching_back_after_a_unique_clash_rebuilds_the_index() {
    run(
        integration_test::scenario_id!(),
        vec![
            patch_with_lens(
                json!({ "dst": "age", "value": 5 }),
                "bin/lenses/set-default.wasm",
            ),
            age_index_failed(),
            SetActiveCollectionVersion {
                version_id: Text::reference("version1"),
                ..Default::default()
            }
            .into(),
            age_index_listed(json!({ "Status": 3, "Reason": "" })),
            query_age(20, json!([{ "age": 20, "name": "Andy" }])),
            explain_age(20, 1),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.query.index.unique_clash")]
fn migration_keeping_unique_values_distinct_keeps_the_index() {
    run(
        integration_test::scenario_id!(),
        vec![
            patch_with_lens(
                json!({ "field": "age", "value": 5 }),
                "bin/lenses/increment.wasm",
            ),
            age_index_listed(json!({ "Status": 3, "Reason": "" })),
            query_age(25, json!([{ "age": 25, "name": "Andy" }])),
            explain_age(25, 1),
        ],
    );
}
