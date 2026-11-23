//! Truncating a collection leaves every other collection's documents and
//! index entries in place.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request, Truncate};
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

/// Collections truncated around the indexed one that is kept. Short IDs are
/// handed out in order, so these take every short ID up past 118: a short ID
/// below 240 is written as one raw byte, and 100 (`d`) and 118 (`v`) spell
/// the families that hold every collection's documents and their versions.
const TRUNCATED: usize = 125;

fn truncated(n: usize) -> String {
    format!("Truncated{n}")
}

fn sdl() -> String {
    let mut sdl = String::from("type Kept { name: String @index }\n");
    for n in 0..TRUNCATED {
        sdl.push_str(&format!(
            "type {} {{ name: String @index }}\n",
            truncated(n)
        ));
    }
    sdl
}

fn by_name(collection: &str, name: &str, expected: serde_json::Value) -> ActionInstance<Actions> {
    Request {
        request: format!(
            r#"query {{ {collection}(filter: {{name: {{_eq: "{name}"}}}}) {{ name }} }}"#
        )
        .into(),
        expected_data: serde_json::Value::Object(
            [(collection.to_owned(), expected)].into_iter().collect(),
        ),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate.other_collections")]
fn truncate_leaves_other_collections_indexes_intact() {
    let mut actions: Vec<ActionInstance<Actions>> = vec![
        AddCollection {
            sdl: sdl().into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Kept".into(),
            doc_map: Some(json!({ "name": "Kept" })),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: truncated(0),
            doc_map: Some(json!({ "name": "Gone" })),
            ..Default::default()
        }
        .into(),
    ];
    actions.extend((0..TRUNCATED).map(|n| {
        Truncate {
            collection: truncated(n),
            ..Default::default()
        }
        .into()
    }));
    actions.push(by_name(&truncated(0), "Gone", json!([])));
    actions.push(by_name("Kept", "Kept", json!([{ "name": "Kept" }])));
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}
