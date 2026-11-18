//! A lens linked to an inactive version leaves the collection's active
//! version where it was: still bound to the collection's name, still queryable.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddLens, PatchCollection, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

/// `Users` with `email` added: the version that `phone` then replaces.
const V2: &str = "bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu";

/// `email` makes v2 and `phone` makes v3, the active version. A patch then
/// links a lens to v2, now inactive, by its version ID. `Users` still answers
/// a query, at v3.
#[test]
#[mantra_macros::req_test("go.collection_version.migrations.patch_transform")]
fn col_version_update_replace_inactive_version_transform_keeps_active_version() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
                    ]
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "phone", "Kind": 11} }
                    ]
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "name", "value": "Fred" }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: Text::concat([
                    Text::from(format!(
                        r#"
                    [
                        {{ "op": "add", "path": "/{V2}/PreviousVersion/Transform", "value": ""#
                    )),
                    Text::reference("lens0"),
                    Text::from(
                        r#"" }
                    ]
                "#,
                    ),
                ]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                            phone
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
