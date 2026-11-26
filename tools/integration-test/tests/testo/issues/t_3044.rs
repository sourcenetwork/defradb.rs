use integration_test::actions::shared::{IndexField, IndexSpec, Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, GetCollections, NewIndex, PatchCollection, Request,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.partial_lens_read")]
fn indexed_collection_query_after_two_lenses_returns_migrated_fields() {
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
            GetCollections {
                common: Common {
                    capture: captures([("version0", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "Users".into(),
                index: IndexSpec {
                    name: "idx_name".into(),
                    fields: vec![IndexField {
                        name: "name".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "verified", "Kind": "Boolean"} }
                    ]
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    capture: captures([("version1", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} }
                    ]
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    capture: captures([("version2", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: Text::reference("version0"),
                destination_version_id: Text::reference("version1"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "verified", "value": true }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: Text::reference("version1"),
                destination_version_id: Text::reference("version2"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "email", "value": "ilovewasm@source.com" }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users(filter: {name: {_eq: "John"}}) {
                            name
                            verified
                            email
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{
                        "email": "ilovewasm@source.com",
                        "name": "John",
                        "verified": true,
                    }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
