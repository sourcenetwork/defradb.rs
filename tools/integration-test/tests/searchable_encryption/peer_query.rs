//! Generated from the conformance scenarios of tests/integration/searchable_encryption/peer_query_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, NewNode, Request, WaitForPeersEvents, WaitForSESync,
    WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.searchable_encryption.query.schema")]
fn doc_encryption_if_there_is_index_but_on_another_field_encrypted_query_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @encryptedIndex
                        age: Int 
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @encryptedIndex
                        age: Int 
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {age: {_eq: 21}}) {
                            docIDs
                        }
                    }
                "#
                .into(),
                expected_error: Some(r#"Argument "filter" has invalid value"#.into()),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.searchable_encryption.query.schema")]
fn doc_encryption_if_there_is_no_index_encrypted_query_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int 
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int 
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {age: {_eq: 21}}) {
                            docIDs
                        }
                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Cannot query field "encrypted_User" on type "Query"."#.into(),
                ),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.searchable_encryption.query.results")]
fn doc_encryption_peer_with_multiple_docs_should_filter_correctly() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @encryptedIndex
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @encryptedIndex
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 30 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc1")),
                common: Common { node: Some("db0".into()), capture: captures([("head1", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 30 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc2")),
                common: Common { node: Some("db0".into()), capture: captures([("head2", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSESync {
                doc_ids: vec![Text::reference("doc0"), Text::reference("doc1"), Text::reference("doc2")],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {age: {_eq: 21}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::unordered(json!([m::r("doc0")])) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {age: {_eq: 30}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "encrypted_User": [{ "docIDs": m::unordered(json!([m::r("doc1"), m::r("doc2")])) }],
                }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {age: {_eq: 33}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": [] }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.searchable_encryption.query.results")]
fn doc_encryption_peer_with_multiple_encrypted_fields_query_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @encryptedIndex
                        age: Int @encryptedIndex
                        city: String @encryptedIndex
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @encryptedIndex
                        age: Int @encryptedIndex
                        city: String @encryptedIndex
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 25, "city": "New York", "verified": true })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSESync {
                doc_ids: vec![Text::reference("doc0")],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {name: {_eq: "John"}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::unordered(json!([m::r("doc0")])) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {age: {_eq: 25}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::unordered(json!([m::r("doc0")])) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {city: {_eq: "New York"}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::unordered(json!([m::r("doc0")])) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.searchable_encryption.query.results")]
fn doc_encryption_peer_with_query_on_multiple_fields_should_return_intersection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @encryptedIndex
                        age: Int @encryptedIndex
                        city: String @encryptedIndex
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @encryptedIndex
                        age: Int @encryptedIndex
                        city: String @encryptedIndex
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc1")),
                common: Common { node: Some("db0".into()), capture: captures([("head1", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 30 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc2")),
                common: Common { node: Some("db0".into()), capture: captures([("head2", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSESync {
                doc_ids: vec![Text::reference("doc0"), Text::reference("doc1"), Text::reference("doc2")],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {name: {_eq: "John"}, age: {_eq: 30}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::unordered(json!([m::r("doc1")])) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {name: {_eq: "Bob"}, age: {_eq: 21}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::length(0) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.searchable_encryption.query.results")]
fn doc_encryption_peer_with_simple_request_should_fetch_successfully() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @encryptedIndex
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @encryptedIndex
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSESync {
                doc_ids: vec![Text::reference("doc0")],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {age: {_eq: 21}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::unordered(json!([m::r("doc0")])) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
