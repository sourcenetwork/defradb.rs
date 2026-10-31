//! Generated from the conformance scenarios of tests/integration/collection_version/one_many_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections, Request};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.relations.one_many")]
fn collection_version_one_many_primary() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        dogs: [Dog]
                    }
                    type Dog {
                        name: String
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
                                    "Name": "dogs",
                                    "Kind": {
                                        "Array": true,
                                        "CollectionID": "bafyreicdzapaezl3xg22wjg5mu3in7wooqpn52jzazrigsayuan7eqlccq",
                                    },
                                    "Typ": 0,
                                    "IsPrimary": false,
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
                                        "CollectionID": "bafyreibhpgygzsmki22sql5ejzcojrrxbc5iuhpydhdzxul5w2znc7zrgu",
                                    },
                                    "Typ": 0,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "dog_user",
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
#[mantra_macros::req_test("go.collection_version.relations.one_many")]
fn collection_version_one_many_self_reference_many_field_lexographically_first() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        b: User
                        a: [User]
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
                            "Name": "User",
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
                                    "Name": "_bID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "a",
                                    "Kind": { "RelativeID": "", "Array": true },
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "b",
                                    "Kind": { "RelativeID": "", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": true,
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

#[test]
#[mantra_macros::req_test("go.collection_version.relations.one_many")]
fn collection_version_one_many_self_reference_one_field_lexographically_first() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        a: User
                        b: [User]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "User",
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
                                    "FieldID": "bafyreihqzhiz3iwro4jozp6kphq4sosg6ccoqcbiaf7rg5dmvea7aux55a",
                                })),
                                m::object(json!({
                                    "Name": "_aID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                    "FieldID": "bafyreibvvrhhuptmxjfzdk4glmw7aj2w6kv4cybrvg25ynrg6evas4j3ha",
                                })),
                                m::object(json!({
                                    "Name": "a",
                                    "Kind": { "RelativeID": "", "Array": false },
                                    "Typ": 0,
                                    "IsPrimary": true,
                                    "DefaultValue": null,
                                    "RelationName": "user_user",
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "b",
                                    "Kind": { "RelativeID": "", "Array": true },
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

#[test]
#[mantra_macros::req_test("go.collection_version.relations.one_many")]
fn collection_version_one_many_self_using_actual_name() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        boss: User @primary
                        minions: [User]
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
                            "Name": "User",
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
                                    "Name": "_bossID",
                                    "Kind": 1,
                                    "Typ": 1,
                                    "IsPrimary": true,
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
                                    "Name": "minions",
                                    "Kind": { "RelativeID": "", "Array": true },
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
            Request {
                request: r#"
                    query {
                        __type (name: "User") {
                            name
                            fields {
                                name
                                type {
                                    name
                                    kind
                                    ofType { name kind }
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "__type": {
                        "fields": [
                            {
                                "name": "AVG",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Float" },
                                },
                            },
                            {
                                "name": "COUNT",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Int" },
                                },
                            },
                            {
                                "name": "GROUP",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "User" },
                                },
                            },
                            {
                                "name": "MAX",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "MIN",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "SIMILARITY",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "SUM",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Float" },
                                },
                            },
                            {
                                "name": "_bossID",
                                "type": { "kind": "SCALAR", "name": "ID", "ofType": null },
                            },
                            {
                                "name": "_deleted",
                                "type": { "kind": "SCALAR", "name": "Boolean", "ofType": null },
                            },
                            {
                                "name": "_docID",
                                "type": { "kind": "SCALAR", "name": "ID", "ofType": null },
                            },
                            {
                                "name": "_version",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "Commit" },
                                },
                            },
                            {
                                "name": "boss",
                                "type": { "kind": "OBJECT", "name": "User", "ofType": null },
                            },
                            {
                                "name": "minions",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "User" },
                                },
                            },
                        ],
                        "name": "User",
                    },
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
