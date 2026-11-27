//! `@immutable` on a single-object relation field holds its `_xID`.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::common::CLIENTS;

fn add_author(name: &str, capture: &str) -> super::common::Action {
    AddDoc {
        collection: "Author".into(),
        doc_map: Some(json!({ "name": name })),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn update_book(field: &str, author: &str) -> super::common::Action {
    Request {
        request: Text::concat([
            Text::from(r#"mutation { update_Book(docID: ""#),
            Text::reference("book"),
            Text::from(format!(r#"", input: {{{field}: ""#)),
            Text::reference(author),
            Text::from(r#""}) { _docID } }"#),
        ]),
        expected_error: Some("immutable field '_authorID' cannot be changed".into()),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.immutable_relation")]
fn an_immutable_relation_cannot_be_changed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book { title: String author: Author @immutable }
                    type Author { name: String }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            add_author("Herbert", "herbert"),
            add_author("Asimov", "asimov"),
            Request {
                request: Text::concat([
                    Text::from(r#"mutation { add_Book(input: {title: "Dune", author: ""#),
                    Text::reference("herbert"),
                    Text::from(r#""}) { _docID } }"#),
                ]),
                common: Common {
                    capture: captures([("book", "/data/add_Book/0/_docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            update_book("_authorID", "asimov"),
            update_book("author", "asimov"),
            Request {
                request: Text::concat([
                    Text::from(r#"mutation { update_Book(docID: ""#),
                    Text::reference("book"),
                    Text::from(r#"", input: {title: "Dune Messiah"}) { title author { name } } }"#),
                ]),
                expected_data: json!({ "update_Book": [{ "title": "Dune Messiah", "author": { "name": "Herbert" } }] }),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}
