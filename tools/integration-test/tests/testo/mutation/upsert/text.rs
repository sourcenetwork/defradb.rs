//! An upsert that finds its document edits a `@crdt(type: text)` field the way
//! an update does. Text CRDTs are defradb.rs only, so the test runs only
//! against the Rust node.

use integration_test::actions::{Actions, AddCollection, Request};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

fn upsert(body: &str) -> Actions {
    Request {
        request: format!(
            r#"mutation {{ upsert_Note(filter: {{title: {{_eq: "a"}}}}, add: {{title: "a", body: "hello"}}, update: {{body: "{body}"}}) {{ body }} }}"#
        )
        .into(),
        ..Default::default()
    }
    .into()
}

fn body_is(body: &str) -> Actions {
    Request {
        request: "query { Note { body } }".into(),
        expected_data: json!({ "Note": [{ "body": body }] }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.crdt.text_upsert")]
fn an_upsert_edits_an_existing_text() {
    let actions: Vec<Actions> = vec![
        AddCollection {
            sdl: "type Note { title: String body: String @crdt(type: text) }".into(),
            ..Default::default()
        }
        .into(),
        upsert("unused"),
        body_is("hello"),
        upsert("hello world"),
        body_is("hello world"),
        upsert("help world!"),
        body_is("help world!"),
    ];
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}
