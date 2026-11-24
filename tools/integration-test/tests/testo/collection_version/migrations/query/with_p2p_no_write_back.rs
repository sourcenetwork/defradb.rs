//! A query reads a document stored at an older version through the lens and
//! writes nothing back: the document stays at its version in storage.

use integration_test::actions::shared::{IndexField, IndexSpec, Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, ConfigureMigration, NewIndex, NewNode, PatchCollection,
    Request, WaitForPeersEvents, WaitForSync,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

/// The version db0 writes at; db1 patches past it and migrates from it.
const V1: &str = "bafyreiabmrtgxy5dgotuc53gfaamuqhlzugyeetzbuv7s3x6ufmlr5ylga";
const V2: &str = "bafyreidwvvr7kp5rqt7dbgzw55vuueovkjz6b2mlvz3rq2pxf22fqenzdm";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// db1 holds an index on `verified` and a `set-default` migration from v1,
/// and receives John at v1 from db0. Unfiltered and filtered queries read
/// him as `verified: true` through the lens, and his head commit is still at
/// v1 afterwards: neither query wrote the migrated document back.
#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.p2p")]
fn a_query_leaves_a_replicated_old_version_document_at_its_version() {
    let users = r#"
        type Users {
            name: String
            verified: Boolean
        }
    "#;
    let stored_at_v1 = Request {
        request: "query { Users { _version { collectionVersionId } } }".into(),
        expected_data: json!({ "Users": [{ "_version": [{ "collectionVersionId": V1 }] }] }),
        common: on("db1"),
        ..Default::default()
    };
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: users.into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: users.into(),
                common: on("db1"),
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
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: V1.into(),
                destination_version_id: V2.into(),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "verified", "value": true }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "Users".into(),
                index: IndexSpec {
                    name: "UsersByVerified".into(),
                    unique: false,
                    fields: vec![IndexField {
                        name: "verified".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                cid: Some(Text::reference("head")),
                expected_data: json!({}),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            stored_at_v1.clone().into(),
            Request {
                request: "query { Users { name verified } }".into(),
                expected_data: json!({ "Users": [{ "name": "John", "verified": true }] }),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            stored_at_v1.clone().into(),
            Request {
                request: "query { Users(filter: {verified: {_eq: true}}) { name } }".into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            stored_at_v1.into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
