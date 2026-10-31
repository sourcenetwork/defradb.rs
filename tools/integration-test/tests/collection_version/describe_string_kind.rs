//! Generated from the conformance scenarios of tests/integration/collection_version/describe_string_kind_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.get")]
fn add_collection_with_stringified_kinds_round_trips_to_original_versions() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        boss: User @primary
                        minion: User
                    }
                    type Dog {
                        name: String
                        tags: [Int!]
                        owner: User @primary
                    }
                "#.into(),
                expected_results: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "User",
                            "PreviousVersion": null,
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
                                    "Name": "_bossID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "_minionID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "boss",
                                    "Kind": { "RelativeID": "", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "minion",
                                    "Kind": { "RelativeID": "", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                            ],
                        })),
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Dog",
                            "PreviousVersion": null,
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
                                    "Name": "_ownerID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "dog_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "name",
                                    "Kind": 11,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "owner",
                                    "Kind": {
                                        "Array": false,
                                        "CollectionID": "bafyreicuxpdrri4wwdknhbchhdii6tu4myqlhspv3s2c3pci7jt7qc3zua",
                                    },
                                    "Typ": 0,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "dog_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "tags",
                                    "Kind": 5,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
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
#[mantra_macros::req_test("go.collection_version.get")]
fn get_collection_version_renders_and_round_trips_stringified_kinds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        boss: User @primary
                        minion: User
                    }
                    type Dog {
                        name: String
                        tags: [Int!]
                        owner: User @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Dog".into()),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Dog",
                            "PreviousVersion": null,
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
                                    "Name": "_ownerID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "dog_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "name",
                                    "Kind": 11,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "owner",
                                    "Kind": {
                                        "Array": false,
                                        "CollectionID": "bafyreicuxpdrri4wwdknhbchhdii6tu4myqlhspv3s2c3pci7jt7qc3zua",
                                    },
                                    "Typ": 0,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "dog_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "tags",
                                    "Kind": 5,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
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
            GetCollections {
                collection_name: Some("User".into()),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "User",
                            "PreviousVersion": null,
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
                                    "Name": "_bossID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "_minionID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "boss",
                                    "Kind": { "RelativeID": "", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "minion",
                                    "Kind": { "RelativeID": "", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
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
