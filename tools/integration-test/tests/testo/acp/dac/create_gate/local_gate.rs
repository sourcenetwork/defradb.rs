//! A governed collection whose policy gates creates, under the local ACP:
//! `create: author->publish` is decided in the create's transaction over the
//! relationship the document's bound field carries.

use integration_test::actions::{AddDACActorRelationship, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::common::{add_collection, add_policy, sdl, Action, CLIENTS, LIBRARY};

const BOUND: &str = r#"
    type Book @governed(root: "root-a") @policy(id: "{library}", resource: "book") {
        title: String
        author: Author @immutable @policy(relation: "author")
    }
    type Author @policy(id: "{library}", resource: "author") {
        name: String
    }
"#;

/// Identity 1 adds the library, the collections and the author Mike, and
/// makes identity 2 Mike's publisher.
fn library_with_a_publisher() -> Vec<Action> {
    vec![
        add_policy(LIBRARY, "library"),
        add_collection(BOUND, &["library"], None),
        AddDoc {
            collection: "Author".into(),
            doc_map: Some(json!({ "name": "Mike" })),
            common: Common {
                identity_seed: Some(1),
                capture: captures([("mike", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDACActorRelationship {
            collection: "Author".into(),
            doc_id: Text::reference("mike"),
            relation: "publisher".into(),
            target_identity_seed: Some(2),
            expected_data: json!({ "existedAlready": false }),
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

/// `add_Book` of Dune by Mike, as identity `seed` (anonymous without one).
fn add_dune(seed: Option<u64>, expected_error: Option<&str>) -> Action {
    Request {
        request: sdl(
            r#"mutation { add_Book(input: {title: "Dune", author: "{mike}"}) { title } }"#,
            &["mike"],
        ),
        expected_data: if expected_error.is_none() {
            json!({ "add_Book": [{ "title": "Dune" }] })
        } else {
            serde_json::Value::Null
        },
        expected_error: expected_error.map(Into::into),
        common: Common {
            identity_seed: seed,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.local")]
fn a_refused_create_stores_nothing_and_an_admitted_one_takes_its_id() {
    let mut actions = library_with_a_publisher();
    actions.extend([
        // Identity 3 may not publish for Mike: refused in the transaction,
        // so the document ID stays free.
        add_dune(Some(3), Some("not authorized to create document ")),
        // Identity 2 may: the same content gets the same ID, which is free.
        add_dune(Some(2), None),
        // The stored relationship lets Mike's publisher read the book.
        Request {
            request: r#"query { Book { title } }"#.into(),
            expected_data: json!({ "Book": [{ "title": "Dune" }] }),
            common: Common {
                identity_seed: Some(2),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query { Book { title } }"#.into(),
            expected_data: json!({ "Book": [] }),
            common: Common {
                identity_seed: Some(3),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.local")]
fn an_anonymous_create_in_a_gated_collection_is_refused() {
    let mut actions = library_with_a_publisher();
    actions.push(add_dune(
        None,
        Some("not authorized to create a document in collection Book without an identity"),
    ));
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}
