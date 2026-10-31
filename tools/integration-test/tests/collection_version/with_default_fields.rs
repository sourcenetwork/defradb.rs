//! Generated from the conformance scenarios of tests/integration/collection_version/with_default_fields_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.define.default_values")]
fn collection_version_with_default_field_values() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        active: Boolean @default(value: true)
                        created: DateTime @default(value: "2000-07-23T03:00:00.000Z")
                        name: String @default(value: "Bob")
                        age: Int @default(value: 10)
                        points: Float @default(value: 30)
                        metadata: JSON @default(value: "{\"value\":1}")
                        image: Blob @default(value: "ff0099")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "Users",
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "PreviousVersion": m::optional(json!(null)),
                            "Fields": [
                                m::object(json!({
                                    "Name": "_docID",
                                    "Kind": 1,
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "active",
                                    "Kind": 2,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": true,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "age",
                                    "Kind": 4,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": 10,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "created",
                                    "Kind": 10,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": "2000-07-23T03:00:00Z",
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "image",
                                    "Kind": 13,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": "ff0099",
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "metadata",
                                    "Kind": 14,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": r#"{"value":1}"#,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "name",
                                    "Kind": 11,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": "Bob",
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "points",
                                    "Kind": 6,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": 30.0,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                            ],
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

#[test]
#[mantra_macros::req_test("go.collection_version.define.default_values")]
fn collection_version_with_default_field_value_no_arg_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name: String @default
                    }
                "#
            .into(),
            expected_error: Some("default value must specify one argument. Field: name".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.default_values")]
fn collection_version_with_default_field_value_on_list_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type User {
                        names: [String] @default(value: "Bob")
                    }
                "#
            .into(),
            expected_error: Some(
                "default value is not allowed for this field type. Name: names, Type: List".into(),
            ),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.default_values")]
fn collection_version_with_default_field_value_on_relation_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type User {
                        friend: User @default(value: "Bob")
                    }
                "#
            .into(),
            expected_error: Some(
                "default value is not allowed for this field type. Name: friend, Type: User".into(),
            ),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.default_values")]
fn collection_version_with_incorrect_default_field_value_type_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        active: Boolean @default(value: 10)
                    }
                "#.into(),
                expected_error: Some("default value is invalid. Field: active, Expected: Boolean, Actual: Int, Value: 10".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.default_values")]
fn collection_version_with_invalid_default_field_value_type_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        active: Boolean @default(value: invalid)
                    }
                "#.into(),
                expected_error: Some("default value is invalid. Field: active, Expected: Boolean, Actual: Enum, Value: invalid".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.default_values")]
fn collection_version_with_unknown_default_field_arg_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        active: Boolean @default(bool: true)
                    }
                "#
            .into(),
            expected_error: Some(r#"Unknown argument "bool" on directive "@default""#.into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}
