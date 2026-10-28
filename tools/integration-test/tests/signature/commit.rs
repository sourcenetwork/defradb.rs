//! Generated from the conformance scenarios of tests/integration/signature/commit_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, DeleteDoc, NodeOpenSigningEd25519, NodeOpenSigningSecp256k1,
    PeerEventsStart, Request, SaveDoc, UpdateDoc, WaitForUpdate, WaitForUpdates,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.signature.signer")]
fn signature_with_client_identity_should_use_it_for_signing() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningSecp256k1 {
                ..Default::default()
            }
            .into(),
            NodeOpenSigningEd25519 {
                node_identity_seed: Some(1),
                common: Common { node: Some("client".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { identity_node: Some("client".into()), retry: true, capture: captures([("doc", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_map: json!({ "age": 23 }),
                doc_id: Text::reference("doc"),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_map: json!({ "name": "John Doe" }),
                doc_id: Text::reference("doc"),
                common: Common { identity_node: Some("client".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(filter: {fieldName: {_eq: "_C"}}, order: {height: DESC}) {
                            height
                            signature {
                                type
                                identity
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "height": 3,
                            "signature": {
                                "type": "EdDSA",
                                "identity": "6f1581709bb7b1ef030d210db18e3b0ba1c776fba65d8cdaad05415142d189f8",
                            },
                        },
                        {
                            "height": 2,
                            "signature": {
                                "type": "ES256K",
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                            },
                        },
                        {
                            "height": 1,
                            "signature": {
                                "type": "EdDSA",
                                "identity": "6f1581709bb7b1ef030d210db18e3b0ba1c776fba65d8cdaad05415142d189f8",
                            },
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        skip: Some("Skipping test because signing with client identity is not supported yet"),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.signature.commits.create")]
fn signature_with_commit_query_should_be_hex_encoded() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningSecp256k1 {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common { retry: true, capture: captures([("doc", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                common: Common { capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            fieldName
                            signature {
                                type
                                identity
                                value
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "fieldName": "age",
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                                "value": "MEQCIBHTFj1KRqJovwTIG+nud+lZRQFuCvuTE2myk+Ct3bFzAiBR+BNAomLIRe5niZytTqtesMsWj2gMzzK/R9nYZo7jTg==",
                            },
                        },
                        {
                            "fieldName": "name",
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                                "value": "MEUCIQDnIrruOt9MK2wtHfq0kS3itwINo/TC4z/SU/h97CGo2AIgSYR1/hc2cqiE56hN9m9JgAfcx36sVns8Gg1oed5n6cw=",
                            },
                        },
                        {
                            "fieldName": "_C",
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                                "value": m::nonempty_string(),
                            },
                        },
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
#[mantra_macros::req_test("go.signature.commits.create")]
fn signature_with_commit_query_should_include_signature_data() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningSecp256k1 {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common { retry: true, capture: captures([("doc", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                common: Common { capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            fieldName
                            signature {
                                type
                                identity
                                value
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "fieldName": "age",
                            "signature": {
                                "identity": m::all(json!([m::nonempty_string(), m::same("sameIdentity")])),
                                "type": "ES256K",
                                "value": "MEQCIBHTFj1KRqJovwTIG+nud+lZRQFuCvuTE2myk+Ct3bFzAiBR+BNAomLIRe5niZytTqtesMsWj2gMzzK/R9nYZo7jTg==",
                            },
                        },
                        {
                            "fieldName": "name",
                            "signature": {
                                "identity": m::same("sameIdentity"),
                                "type": "ES256K",
                                "value": "MEUCIQDnIrruOt9MK2wtHfq0kS3itwINo/TC4z/SU/h97CGo2AIgSYR1/hc2cqiE56hN9m9JgAfcx36sVns8Gg1oed5n6cw=",
                            },
                        },
                        {
                            "fieldName": "_C",
                            "signature": {
                                "identity": m::same("sameIdentity"),
                                "type": "ES256K",
                                "value": m::nonempty_string(),
                            },
                        },
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
#[mantra_macros::req_test("go.signature.commits.update_delete")]
fn signature_with_deleted_doc_and_commit_query_should_include_signature_data() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningSecp256k1 {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { retry: true, capture: captures([("doc", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                common: Common { capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                common: Common { capture: captures([("head1", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(order: {height: DESC}, filter: {fieldName: {_eq: "_C"}}) {
                            fieldName
                            height
                            signature {
                                type
                                identity
                                value
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "fieldName": "_C",
                            "height": 2,
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                                "value": m::unique("uniqueSignature"),
                            },
                        },
                        {
                            "fieldName": "_C",
                            "height": 1,
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                                "value": m::unique("uniqueSignature"),
                            },
                        },
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
#[mantra_macros::req_test("go.signature.commits.create")]
fn signature_with_ed25519_key_type_should_include_signature_data() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningEd25519 {
                node_identity_seed: Some(0),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common { retry: true, capture: captures([("doc", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                common: Common { capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            fieldName
                            signature {
                                type
                                identity
                                value
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "fieldName": "age",
                            "signature": {
                                "identity": "97f3bd871315281e8b83edc7a9fd0541066154449070ccdb3cdd42cf69ccde88",
                                "type": "EdDSA",
                                "value": "/Da2mRMGCf15Gc8uiDs03Qfy/531uwjJj40rY1FmXjnwzTSMWUCtN7Eimwws3fFd/hykKX5M8GA08vedVtZEDA==",
                            },
                        },
                        {
                            "fieldName": "name",
                            "signature": {
                                "identity": "97f3bd871315281e8b83edc7a9fd0541066154449070ccdb3cdd42cf69ccde88",
                                "type": "EdDSA",
                                "value": "dmheRrDmXWMkF4vdwzAlRZIv9POfIE34wtDnkItSSa2BSxQd4RbYfOaREGKTtG5VuggM6hRjMDzmWD94xFsEBw==",
                            },
                        },
                        {
                            "fieldName": "_C",
                            "signature": {
                                "identity": "97f3bd871315281e8b83edc7a9fd0541066154449070ccdb3cdd42cf69ccde88",
                                "type": "EdDSA",
                                "value": m::nonempty_string(),
                            },
                        },
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
#[mantra_macros::req_test("go.signature.signer")]
fn signature_with_per_op_signing_disabled_should_not_sign_any_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningSecp256k1 {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            SaveDoc {
                collection: "Users".into(),
                doc_map: json!({ "age": 21, "name": "John" }),
                enable_signing: Some(false),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            fieldName
                            signature {
                                type
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        { "fieldName": "age", "signature": null },
                        { "fieldName": "name", "signature": null },
                        { "fieldName": "_C", "signature": null },
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
#[mantra_macros::req_test("go.signature.signer")]
fn signature_with_per_op_signing_override_on_update() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningSecp256k1 {
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
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            SaveDoc {
                collection: "Users".into(),
                doc_map: json!({ "name": "John" }),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                ..Default::default()
            }
            .into(),
            SaveDoc {
                collection: "Users".into(),
                doc_id: Some(Text::reference("doc")),
                doc_map: json!({ "name": "John Doe" }),
                enable_signing: Some(false),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(order: {height: DESC}, filter: {fieldName: {_eq: "_C"}}) {
                            height
                            signature {
                                type
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "height": 2, "signature": null },
                        { "height": 1, "signature": { "type": "ES256K" } },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            SaveDoc {
                collection: "Users".into(),
                doc_id: Some(Text::reference("doc")),
                doc_map: json!({ "name": "John Doe Junior" }),
                enable_signing: Some(true),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(order: {height: DESC}, filter: {fieldName: {_eq: "_C"}}) {
                            height
                            signature {
                                type
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "height": 3, "signature": { "type": "ES256K" } },
                        { "height": 2, "signature": null },
                        { "height": 1, "signature": { "type": "ES256K" } },
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
#[mantra_macros::req_test("go.signature.commits.update_delete")]
fn signature_with_updated_docs_and_commit_query_should_sign_only_first_field_blocks() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningSecp256k1 {
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
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "name": "John Doe" }),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "name": "John Doe Junior" }),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc")],
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(order: {height: DESC}) {
                            fieldName
                            height
                            signature {
                                type
                                identity
                                value
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "fieldName": "name", "height": 3, "signature": null },
                        {
                            "fieldName": "_C",
                            "height": 3,
                            "signature": {
                                "identity": m::same("sameIdentity"),
                                "type": "ES256K",
                                "value": m::unique("uniqueSignature"),
                            },
                        },
                        { "fieldName": "name", "height": 2, "signature": null },
                        {
                            "fieldName": "_C",
                            "height": 2,
                            "signature": {
                                "identity": m::same("sameIdentity"),
                                "type": "ES256K",
                                "value": m::unique("uniqueSignature"),
                            },
                        },
                        {
                            "fieldName": "name",
                            "height": 1,
                            "signature": {
                                "identity": m::same("sameIdentity"),
                                "type": "ES256K",
                                "value": m::unique("uniqueSignature"),
                            },
                        },
                        {
                            "fieldName": "_C",
                            "height": 1,
                            "signature": {
                                "identity": m::same("sameIdentity"),
                                "type": "ES256K",
                                "value": m::unique("uniqueSignature"),
                            },
                        },
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
