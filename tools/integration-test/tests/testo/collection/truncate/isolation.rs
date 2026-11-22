use integration_test::actions::{
    AddCollection, AddDoc, DeleteIndex, GetCollections, Request, Truncate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.collection.truncate_isolation")]
#[mantra_macros::req_test("defra.index.keyspace_isolation")]
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
        GetCollections {
            collection_name: Some("Kept".into()),
            filtered: true,
            common: Common {
                capture: captures([("kept_versions", "/collections")]),
                ..Default::default()
            },
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
                collection: name.clone(),
                doc_map: Some(json!({ "name": "removed" })),
                ..Default::default()
            }
            .into(),
        );
        if n == 99 || n == 117 {
            actions.push(
                Request {
                    request: format!(
                        "query {{ {name}(filter: {{name: {{_eq: \"removed\"}}}}) {{ name }} }}"
                    )
                    .into(),
                    expected_data: json!({ (name.clone()): [{"name": "removed"}] }),
                    ..Default::default()
                }
                .into(),
            );
            actions.push(
                DeleteIndex {
                    collection: name.clone(),
                    index_name: format!("{name}_name_ASC"),
                    ..Default::default()
                }
                .into(),
            );
            actions.push(
                Request {
                    request: format!("query {{ Kept(filter: {{name: {{_eq: \"retained\"}}}}) {{ name }} {name} {{ name }} }}").into(),
                    expected_data: json!({"Kept": [{"name": "retained"}], (name.clone()): [{"name": "removed"}]}),
                    ..Default::default()
                }
                .into(),
            );
        }
        actions.push(
            Truncate {
                collection: name,
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        GetCollections {
            collection_name: Some("Kept".into()),
            filtered: true,
            expected_data: json!({"collections": m::r("kept_versions")}),
            ..Default::default()
        }
        .into(),
    );
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
