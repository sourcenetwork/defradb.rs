//! `@policy(relation:)` on a relation field, checked when the collection is
//! added.

use super::common::{add_collection, add_policy, CLIENTS, LIBRARY};
use integration_test::actions::Request;
use integration_test::test_case::TestCase;
use serde_json::json;

const BOUND: &str = r#"
    type Book @governed(root: "root-a") @policy(id: "{library}", resource: "book") {
        title: String
        author: Author @immutable @policy(relation: "author")
    }
    type Author @policy(id: "{library}", resource: "author") {
        name: String
    }
"#;

/// Each SDL in `cases` is refused with its error, and creates nothing.
fn refused(id: &'static str, cases: &[(&str, &str)]) {
    let mut actions = vec![add_policy(LIBRARY, "library")];
    for (template, error) in cases {
        actions.push(add_collection(template, &["library"], Some(error)));
    }
    actions.push(
        Request {
            request: r#"query { __type(name: "Book") { name } }"#.into(),
            expected_data: json!({ "__type": null }),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id,
        actions,
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.binding")]
fn a_binding_on_a_governed_primary_immutable_field_is_accepted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_policy(LIBRARY, "library"),
            add_collection(BOUND, &["library"], None),
            Request {
                request: r#"query { __type(name: "Book") { name } }"#.into(),
                expected_data: json!({ "__type": { "name": "Book" } }),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.binding")]
fn a_binding_outside_a_governed_collection_is_refused() {
    refused(
        integration_test::scenario_id!(),
        &[(
            &BOUND.replace(r#"@governed(root: "root-a") "#, ""),
            "invalid @policy(relation:) on field Book.author: the collection must be governed (@governed(root:))",
        )],
    );
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.binding")]
fn a_binding_on_the_secondary_side_is_refused() {
    refused(
        integration_test::scenario_id!(),
        &[(r#"
        type Book @governed(root: "root-a") @policy(id: "{library}", resource: "book") {
            title: String
            author: Author @policy(relation: "author")
        }
        type Author @policy(id: "{library}", resource: "author") {
            name: String
            book: Book @primary
        }
        "#,
        "invalid @policy(relation:) on field Book.author: the binding must be on the primary side of the relation, which stores the related document ID",
        )],
    );
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.binding")]
fn a_binding_on_a_mutable_field_is_refused() {
    refused(
        integration_test::scenario_id!(),
        &[(
            &BOUND.replace("@immutable ", ""),
            "invalid @policy(relation:) on field Book.author: the field must be @immutable",
        )],
    );
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.binding")]
fn a_binding_to_a_collection_under_another_policy_is_refused() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_policy(LIBRARY, "library"),
            add_policy(&LIBRARY.replace("name: library", "name: other"), "other"),
            add_collection(
                &BOUND.replace(
                    r#"type Author @policy(id: "{library}""#,
                    r#"type Author @policy(id: "{other}""#,
                ),
                &["library", "other"],
                Some("invalid @policy(relation:) on field Book.author: the related collection Author must have the same policy"),
            ),
        ],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.binding")]
fn a_binding_to_a_relation_of_another_type_is_refused() {
    refused(
        integration_test::scenario_id!(),
        &[
            (
                &BOUND.replace(r#"@policy(relation: "author")"#, r#"@policy(relation: "editor")"#),
                "invalid @policy(relation:) on field Book.author: relation types must be exactly the related collection's resource",
            ),
            (
                &BOUND.replace(r#"@policy(relation: "author")"#, r#"@policy(relation: "read")"#),
                "invalid @policy(relation:) on field Book.author: relation does not exist on the resource",
            ),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.binding")]
fn a_binding_on_a_scalar_field_is_refused() {
    refused(
        integration_test::scenario_id!(),
        &[(
            &BOUND.replace(
                "title: String",
                r#"title: String @policy(relation: "author")"#,
            ),
            "@policy(relation:) requires a single-object relation field. Field: Book.title",
        )],
    );
}
