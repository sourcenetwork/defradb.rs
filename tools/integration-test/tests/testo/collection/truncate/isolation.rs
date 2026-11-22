use integration_test::actions::{AddCollection, AddDoc, Request, Truncate};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.collection.truncate_isolation")]
fn truncate_preserves_other_collections() {
    let mut actions = vec![
        AddCollection {
            sdl: "type Kept { name: String @index }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Kept".into(),
            doc_map: Some(json!({ "name": "retained" })),
            ..Default::default()
        }
        .into(),
    ];
    for n in 0..125 {
        let name = format!("Filler{n}");
        actions.push(
            AddCollection {
                sdl: format!("type {name} {{ name: String @index }}").into(),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddDoc {
                collection: name.clone().into(),
                doc_map: Some(json!({ "name": "removed" })),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            Truncate {
                collection: name.into(),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        Request {
            request:
                "query { Kept(filter: {name: {_eq: \"retained\"}}) { name } Filler124 { name } }"
                    .into(),
            expected_data: json!({ "Kept": [{ "name": "retained" }], "Filler124": [] }),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}
