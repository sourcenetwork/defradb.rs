//! Generated from the conformance scenarios of tests/integration/index/patch_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.index.definition.patch-immutable")]
fn patch_collection_add_secondary_index_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        email: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "User/Indexes/-",
							"value": {
                                "Fields": [
                                    {
                                        "Name": "email",
                                        "Descending": false
                                    }
                                ],
                                "Unique": false
                            }
						}
					]
				"#
                .into(),
                expected_error: Some("collection indexes cannot be mutated".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.patch-immutable")]
fn patch_collection_modify_secondary_index_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        email: String @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "User/Indexes",
							"value": [
								{
									"Name": "User_name_ASC",
									"ID": 2,
									"Fields": [
										{
											"Name": "name",
											"Descending": false
										}
									],
									"Unique": false
								}
							]
						}
					]
				"#
                .into(),
                expected_error: Some("collection indexes cannot be mutated".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.patch-immutable")]
fn patch_collection_modify_vector_index_metric_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "User/Indexes/0/KindDescription/Metric",
							"value": "EUCLIDEAN"
						}
					]
				"#
                .into(),
                expected_error: Some("collection indexes cannot be mutated".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.patch-immutable")]
fn patch_collection_remove_secondary_index_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        email: String @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "User/Indexes"
						}
					]
				"#
                .into(),
                expected_error: Some("collection indexes cannot be mutated".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
