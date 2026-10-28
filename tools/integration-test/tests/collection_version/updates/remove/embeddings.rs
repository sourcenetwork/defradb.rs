//! Generated from the conformance scenarios of tests/integration/collection_version/updates/remove/embeddings_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections, PatchCollection};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.valid")]
fn col_version_update_remove_vector_embedding_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["name", "about"], provider: "openai", model: "text-embedding-3-small",  url: "https://api.openai.com/v1")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/Users/VectorEmbeddings/0"
						}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
                            "VectorEmbeddings": [],
                        })),
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
