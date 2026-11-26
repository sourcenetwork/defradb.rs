//! Updates that race a patch adding a lens never miss the lens's transform.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, Parallel, PatchCollection, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.transform_with_version")]
fn collection_migration_updates_during_a_lens_patch_find_the_transform() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type U {
                        name: String
                        n: Int @crdt(type: pcounter) @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u0", "n": 1 })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u1", "n": 1 })),
                common: Common {
                    capture: captures([("doc1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u2", "n": 1 })),
                common: Common {
                    capture: captures([("doc2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u3", "n": 1 })),
                common: Common {
                    capture: captures([("doc3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u4", "n": 1 })),
                common: Common {
                    capture: captures([("doc4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u5", "n": 1 })),
                common: Common {
                    capture: captures([("doc5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u6", "n": 1 })),
                common: Common {
                    capture: captures([("doc6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u7", "n": 1 })),
                common: Common {
                    capture: captures([("doc7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u8", "n": 1 })),
                common: Common {
                    capture: captures([("doc8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u9", "n": 1 })),
                common: Common {
                    capture: captures([("doc9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u10", "n": 1 })),
                common: Common {
                    capture: captures([("doc10", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u11", "n": 1 })),
                common: Common {
                    capture: captures([("doc11", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u12", "n": 1 })),
                common: Common {
                    capture: captures([("doc12", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u13", "n": 1 })),
                common: Common {
                    capture: captures([("doc13", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u14", "n": 1 })),
                common: Common {
                    capture: captures([("doc14", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "u15", "n": 1 })),
                common: Common {
                    capture: captures([("doc15", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Parallel {
                actions: vec![
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
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc0"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc1"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc2"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc3"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc4"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc5"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc6"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc7"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc8"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc9"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc10"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc11"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc12"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc13"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc14"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "U".into(),
                        doc_id: Text::reference("doc15"),
                        doc_map: json!({ "n": 1 }),
                        expected_data: json!(m::object(json!({ "error": "" }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        U(filter: {n: {_eq: 2}}) {
                            verified
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "U": m::length(16) }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
