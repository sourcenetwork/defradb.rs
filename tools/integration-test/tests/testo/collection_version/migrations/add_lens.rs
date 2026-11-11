//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/add_lens_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, AddLens, ListLenses, PatchCollection, Request,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.lens_registry")]
fn add_lens_with_identical_lenses_returns_same_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "name", "value": "Fred" }),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "name", "value": "Fred" }),
                    }]),
                },
                common: Common {
                    capture: captures([("lens1", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListLenses {
                expected_data: json!({
                    "lenses": m::unordered(json!([
                        {
                            "lensID": m::r("lens0"),
                            "modules": [
                                {
                                    "Inverse": false,
                                    "Arguments": { "dst": "name", "value": "Fred" },
                                },
                            ],
                        },
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.lens_registry")]
fn add_lens_with_multiple_lenses_returns_unique_lens_i_ds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "name", "value": "John" }),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "name", "value": "Andy" }),
                    }]),
                },
                common: Common {
                    capture: captures([("lens1", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListLenses {
                expected_data: json!({
                    "lenses": m::unordered(json!([
                        {
                            "lensID": m::r("lens0"),
                            "modules": [
                                {
                                    "Inverse": false,
                                    "Arguments": { "dst": "name", "value": "John" },
                                },
                            ],
                        },
                        {
                            "lensID": m::r("lens1"),
                            "modules": [
                                {
                                    "Inverse": false,
                                    "Arguments": { "dst": "name", "value": "Andy" },
                                },
                            ],
                        },
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.patch_transform")]
fn add_lens_with_patch_collection_transforms_documents() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "name", "value": "Fred" }),
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

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.lens_registry")]
fn add_lens_with_simple_lens_can_be_listed_back() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "name", "value": "Fred" }),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListLenses {
                expected_data: json!({
                    "lenses": m::unordered(json!([
                        {
                            "lensID": m::r("lens0"),
                            "modules": [
                                {
                                    "Inverse": false,
                                    "Arguments": { "dst": "name", "value": "Fred" },
                                },
                            ],
                        },
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
