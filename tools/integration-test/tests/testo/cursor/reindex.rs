use super::{collection, doc_in, names, only, query};
use integration_test::actions::shared::{IndexField, IndexSpec, Lens, LensModule};
use integration_test::actions::{
    Actions, ConfigureMigration, GetCollections, NewIndex, PatchCollection,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

/// Captures the active `Item` version as `name`.
fn version(name: &str) -> Actions {
    GetCollections {
        collection_name: Some("Item".into()),
        filtered: true,
        common: Common {
            capture: captures([(name, "/collections/0/VersionID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// Known failure #2420: after the migration the cursor finds no index on
/// `created_at` ("no supporting index for cursor order field").
#[test]
#[mantra_macros::req_test("defra.cursor.reindex")]
fn a_datetime_index_stays_cursor_visible_after_a_lens_reindex() {
    let stamps = [
        "2026-01-05T18:05:40Z",
        "2026-02-10T09:00:00Z",
        "2026-03-15T12:30:00Z",
        "2026-04-20T08:15:00Z",
        "2026-05-29T13:06:28Z",
    ];
    let mut actions = vec![
        collection("type Item { name: String created_at: DateTime }"),
        version("v1"),
    ];
    actions.extend(stamps.iter().enumerate().map(|(i, stamp)| {
        doc_in(
            "Item",
            json!({ "name": format!("e{i}"), "created_at": stamp }),
        )
    }));
    actions.extend([
        NewIndex {
            collection: "Item".into(),
            index: IndexSpec {
                name: "idx_created_at".into(),
                fields: vec![IndexField {
                    name: "created_at".into(),
                    descending: false,
                }],
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        PatchCollection {
            patch: r#"[{"op":"add","path":"/Item/Fields/-","value":{"Name":"placeholder","Kind":"String"}}]"#
                .into(),
            ..Default::default()
        }
        .into(),
        version("v2"),
        ConfigureMigration {
            source_version_id: Text::reference("v1"),
            destination_version_id: Text::reference("v2"),
            lens: Lens {
                lenses: Some(vec![LensModule {
                    arguments: json!({ "dst": "placeholder", "value": "x" }),
                    path: "bin/lenses/set-default.wasm".into(),
                }]),
            },
            ..Default::default()
        }
        .into(),
        query(
            "query { _cursor { Item(first: 100, order: [{created_at: ASC}]) { name } } }",
            only("Item", names(&["e0", "e1", "e2", "e3", "e4"])),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}
