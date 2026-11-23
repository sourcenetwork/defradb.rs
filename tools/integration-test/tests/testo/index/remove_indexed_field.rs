//! A patch may not remove a field an index reads: the index would stay and
//! fail every later write to the collection.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, ListIndexes, PatchCollection, Request,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

/// Adds the collection, then refuses removing the field at `path` and keeps
/// the index named `index`.
fn refused_removal(
    collection: &str,
    sdl: &str,
    path: &str,
    error: &str,
    index: &str,
) -> Vec<Actions> {
    vec![
        AddCollection {
            sdl: sdl.into(),
            ..Default::default()
        }
        .into(),
        PatchCollection {
            patch: format!(r#"[{{ "op": "remove", "path": "{path}" }}]"#).into(),
            expected_error: Some(error.into()),
            ..Default::default()
        }
        .into(),
        ListIndexes {
            collection: collection.into(),
            expected_data: json!({
                "indexes": m::unordered(json!([m::object(json!({
                    "Description": m::object(json!({ "Name": index })),
                }))])),
            }),
            ..Default::default()
        }
        .into(),
    ]
}

fn add_doc(collection: &str, doc: Value) -> Actions {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(doc),
        ..Default::default()
    }
    .into()
}

fn request(request: &str, expected_data: Value) -> Actions {
    Request {
        request: request.into(),
        expected_data,
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_an_indexed_field_is_refused_and_writes_go_on() {
    let mut actions = refused_removal(
        "Users",
        "type Users { name: String @index  age: Int }",
        "/Users/Fields/2",
        "cannot remove field 'name': index 'Users_name_ASC' depends on it, drop the index first",
        "Users_name_ASC",
    );
    actions.push(add_doc("Users", json!({ "name": "John", "age": 21 })));
    actions.push(request(
        r#"query { Users(filter: {name: {_eq: "John"}}) { name age } }"#,
        json!({ "Users": [{ "name": "John", "age": 21 }] }),
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_a_composite_index_field_is_refused() {
    let mut actions = refused_removal(
        "Users",
        r#"type Users @index(name: "by_name_age", includes: [{field: "name"}, {field: "age"}]) {
            name: String
            age: Int
        }"#,
        "/Users/Fields/1",
        "cannot remove field 'age': index 'by_name_age' depends on it, drop the index first",
        "by_name_age",
    );
    actions.push(add_doc("Users", json!({ "name": "John", "age": 21 })));
    actions.push(request(
        r#"query { Users(filter: {name: {_eq: "John"}, age: {_eq: 21}}) { name age } }"#,
        json!({ "Users": [{ "name": "John", "age": 21 }] }),
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_a_unique_index_field_is_refused_and_uniqueness_holds() {
    let mut actions = refused_removal(
        "Users",
        "type Users { email: String @index(unique: true)  name: String }",
        "/Users/Fields/1",
        "cannot remove field 'email': index 'Users_email_ASC' depends on it, drop the index first",
        "Users_email_ASC",
    );
    actions.push(add_doc("Users", json!({ "email": "j@x", "name": "John" })));
    actions.push(
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "email": "j@x", "name": "Jim" })),
            expected_error: Some(
                "can not index a doc's field(s) that violates unique index".into(),
            ),
            ..Default::default()
        }
        .into(),
    );
    actions.push(request(
        r#"query { Users(filter: {email: {_eq: "j@x"}}) { name } }"#,
        json!({ "Users": [{ "name": "John" }] }),
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_a_json_index_field_is_refused() {
    let mut actions = refused_removal(
        "Users",
        "type Users { custom: JSON @index  name: String }",
        "/Users/Fields/1",
        "cannot remove field 'custom': index 'Users_custom_ASC' depends on it, drop the index first",
        "Users_custom_ASC",
    );
    actions.push(add_doc(
        "Users",
        json!({ "custom": { "level": 3 }, "name": "John" }),
    ));
    actions.push(request(
        r#"query { Users(filter: {custom: {level: {_eq: 3}}}) { name } }"#,
        json!({ "Users": [{ "name": "John" }] }),
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_a_field_no_index_reads_is_accepted() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Users { name: String @index  points: Int }".into(),
            ..Default::default()
        }
        .into(),
        PatchCollection {
            patch: r#"[{ "op": "remove", "path": "/Users/Fields/2" }]"#.into(),
            ..Default::default()
        }
        .into(),
        add_doc("Users", json!({ "name": "John" })),
        request(
            r#"query { Users(filter: {name: {_eq: "John"}}) { name } }"#,
            json!({ "Users": [{ "name": "John" }] }),
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_a_fulltext_field_names_the_field_and_keeps_the_collection_usable() {
    TestCase {
        id: integration_test::scenario_id!(),
        // Go's pinned schema parser does not support @fulltext.
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        actions: vec![
            AddCollection {
                sdl: "type Notes { body: String @fulltext }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "remove", "path": "/Notes/Fields/1" }]"#.into(),
                expected_error: Some(
                    "cannot remove field 'body': a full-text index depends on it, drop the index first. Collection: Notes"
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
            add_doc("Notes", json!({ "body": "still writable" })).into(),
            request(
                r#"query { Notes { body } }"#,
                json!({ "Notes": [{ "body": "still writable" }] }),
            )
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.field_removal")]
fn removing_an_embedding_source_keeps_the_collection_usable() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: r#"
                type Users {
                    name: String
                    name_v: [Float32!] @embedding(fields: ["name"], provider: "ollama", model: "nomic-embed-text", url: "http://localhost:11434/api")
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        PatchCollection {
            patch: r#"[{ "op": "remove", "path": "/Users/Fields/1" }]"#.into(),
            expected_error: Some(
                "the given field does not exist. Embedding generation field: name".into(),
            ),
            ..Default::default()
        }
        .into(),
        // The vector is given, so no embedding is generated.
        add_doc("Users", json!({ "name": "ann", "name_v": [1, 2, 3] })),
        request(
            r#"query { Users { name } }"#,
            json!({ "Users": [{ "name": "ann" }] }),
        ),
    ])
    .execute();
}
