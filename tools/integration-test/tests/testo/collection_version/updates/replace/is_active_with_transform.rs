//! A patch that writes `IsActive` and links a lens through
//! `PreviousVersion/Transform` keeps both: the link is stored and applied,
//! not dropped as the activation is written in place.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, PatchCollection, Request,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

/// Shahzad is stored at v1, then `email` makes v2. One patch sets v2 active
/// again and points its `PreviousVersion` at the `set-default` lens; a query
/// at v2 reads Shahzad through that lens as Fred.
#[test]
#[mantra_macros::req_test("go.collection_version.migrations.patch_transform")]
fn col_version_update_replace_is_active_and_collection_source_transform_keeps_transform() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            ConfigureMigration {
                source_version_id: "bafyreigbatez5rnojqa4ccfqsbguh4ckquxr76elgqij7ckftbxpwqniv4"
                    .into(),
                destination_version_id:
                    "bafyreihiiez4vcgh4rys2zfs74macgwyybchutjslyw2oin747enuywn54".into(),
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
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
                patch: Text::concat([
                    Text::from(
                        r#"
					[
						{ "op": "replace", "path": "/Users/IsActive", "value": true },
						{
							"op": "replace",
							"path": "/Users/PreviousVersion/Transform",
							"value": ""#,
                    ),
                    Text::reference("lens0"),
                    Text::from(
                        r#""
                                }
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
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
