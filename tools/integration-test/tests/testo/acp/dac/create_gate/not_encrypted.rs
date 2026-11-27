//! A bound field is never encrypted: the relationship is derived from its
//! plaintext.

use integration_test::actions::{AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::common::{add_collection, add_policy, CLIENTS, LIBRARY};

const BOUND: &str = r#"
    type Book @governed(root: "root-a") @policy(id: "{library}", resource: "book") {
        title: String
        author: Author @immutable @policy(relation: "author")
    }
    type Author @policy(id: "{library}", resource: "author") {
        name: String
    }
"#;

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.not_encrypted")]
fn encrypting_a_bound_field_is_refused() {
    const REFUSED: &str = "field author is bound to policy relation author and cannot be encrypted";
    // GraphQL types `encryptFields` as the declared fields not starting with
    // `_`, so it can encrypt the bound `_authorID` only through `encrypt`.
    let encrypt_doc = Request {
        request: Text::concat([
            Text::from(r#"mutation { add_Book(encrypt: true, input: {title: "Dune", _authorID: ""#),
            Text::reference("herbert"),
            Text::from(r#""}) { _docID } }"#),
        ]),
        expected_error: Some(REFUSED.into()),
        ..Default::default()
    };
    // The document API can encrypt the whole document too.
    let encrypt_doc_api = AddDoc {
        collection: "Book".into(),
        doc_map: Some(json!({ "title": "Dune", "author": m::r("herbert") })),
        is_doc_encrypted: Some(true),
        expected_error: Some(REFUSED.into()),
        ..Default::default()
    };
    // Naming `_authorID` alone is refused before the binding is looked at:
    // no client can encrypt a built-in field.
    let encrypt_field = AddDoc {
        collection: "Book".into(),
        doc_map: Some(json!({ "title": "Dune", "author": m::r("herbert") })),
        encrypted_fields: Some(vec!["_authorID".into()]),
        expected_error: Some("can not encrypt build-in field. Name: _authorID".into()),
        ..Default::default()
    };
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_policy(LIBRARY, "library"),
            add_collection(BOUND, &["library"], None),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Herbert" })),
                common: Common {
                    capture: captures([("herbert", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            encrypt_doc.into(),
            encrypt_doc_api.into(),
            encrypt_field.into(),
            Request {
                request: r#"query { Book { title } }"#.into(),
                expected_data: json!({ "Book": [] }),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}
