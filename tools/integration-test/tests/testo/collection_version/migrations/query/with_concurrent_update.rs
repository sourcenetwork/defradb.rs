//! Updates that race a patch adding a lens never miss the lens's transform.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    Actions, AddCollection, AddDoc, Parallel, PatchCollection, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const DOCS: usize = 16;

fn no_error() -> serde_json::Value {
    json!(m::object(json!({ "error": "" })))
}

fn add(i: usize) -> Actions {
    AddDoc {
        collection: "U".into(),
        doc_map: Some(json!({ "name": format!("u{i}"), "n": 1 })),
        common: Common {
            capture: captures([(format!("doc{i}").as_str(), "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn update(i: usize) -> Actions {
    UpdateDoc {
        collection: "U".into(),
        doc_id: Text::reference(&format!("doc{i}")),
        doc_map: json!({ "n": 1 }),
        expected_data: no_error(),
        common: Common {
            envelope: true,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn lens_patch() -> Actions {
    PatchCollection {
        patch: r#"
            [
                { "op": "add", "path": "/U/Fields/-", "value": {"Name": "verified", "Kind": "Boolean"} }
            ]
        "#
        .into(),
        lens: Some(Lens {
            lenses: Some(vec![LensModule {
                arguments: json!({ "dst": "verified", "value": true }),
                path: "bin/lenses/set-default.wasm".into(),
            }]),
        }),
        expected_data: no_error(),
        common: Common {
            envelope: true,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.transform_with_version")]
fn collection_migration_updates_during_a_lens_patch_find_the_transform() {
    let mut actions: Vec<Actions> = vec![AddCollection {
        sdl: r#"
            type U {
                name: String
                n: Int @crdt(type: pcounter) @index
            }
        "#
        .into(),
        ..Default::default()
    }
    .into()];
    actions.extend((0..DOCS).map(add));
    actions.push(
        Parallel {
            actions: std::iter::once(lens_patch())
                .chain((0..DOCS).map(update))
                .collect(),
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        Request {
            request: "query { U(filter: {n: {_eq: 2}}) { verified } }".into(),
            expected_data: json!({ "U": vec![json!({ "verified": true }); DOCS] }),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}
