//! A filtered truncate of a `@branchable` collection removes the matching
//! documents and keeps their history, which the collection DAG links.

use integration_test::actions::shared::Filter;
use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, GetCollections,
    NewNode, Request, SyncBranchableCollection, SyncDocs, Truncate, UpdateDoc, WaitForPeersEvents,
    WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn users() -> Actions {
    AddCollection {
        sdl: "type Users @branchable { name: String @index(unique: true) }".into(),
        ..Default::default()
    }
    .into()
}

fn add(name: &str) -> Actions {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(json!({ "name": name })),
        ..Default::default()
    }
    .into()
}

fn truncate_named(name: &str) -> Actions {
    truncate_named_on(None, name)
}

fn truncate_named_on(node: Option<&str>, name: &str) -> Actions {
    Truncate {
        collection: "Users".into(),
        filter: Some(Filter {
            kind: "map".into(),
            value: Some(json!({ "name": { "_eq": name } })),
        }),
        common: Common {
            node: node.map(str::to_owned),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn names(expected: Value) -> Actions {
    Request {
        request: "query { Users(order: {name: ASC}) { name } }".into(),
        expected_data: json!({ "Users": expected }),
        ..Default::default()
    }
    .into()
}

/// Each create writes a field block, a composite block and a collection block.
/// `_commits` lists a document's blocks while the document exists, and every
/// collection block.
fn commit_count(expected: u64) -> Actions {
    Request {
        request: "query { _commits { cid } }".into(),
        expected_data: json!({ "_commits": m::length(expected) }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate.filter_branchable")]
fn filtered_truncate_of_a_branchable_collection_removes_only_the_matches() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            users().into(),
            add("John").into(),
            add("Jane").into(),
            truncate_named("John").into(),
            names(json!([{ "name": "Jane" }])).into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate.filter_branchable")]
fn filtered_truncate_of_a_branchable_collection_keeps_its_history() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            users().into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("john", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            add("Jane").into(),
            commit_count(6).into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { _commits(docID: ""#),
                    Text::reference("john"),
                    Text::from(r#"", filter: {fieldName: {_eq: "_C"}}) { cid } }"#),
                ]),
                expected_data: json!({ "_commits": [{ "cid": m::cid() }] }),
                common: Common {
                    capture: captures([("johnComposite", "/data/_commits/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            truncate_named("John").into(),
            commit_count(4).into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { _commits(cid: ""#),
                    Text::reference("johnComposite"),
                    Text::from(r#"") { cid docID } }"#),
                ]),
                expected_data: json!({
                    "_commits": [{ "cid": m::r("johnComposite"), "docID": m::nonempty_string() }],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { _commits(docID: ""#),
                    Text::reference("john"),
                    Text::from(r#"") { cid } }"#),
                ]),
                expected_data: json!({ "_commits": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate.filter_branchable")]
fn a_document_truncated_from_a_branchable_collection_can_be_added_again() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            users().into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("john", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            add("Jane").into(),
            truncate_named("John").into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                expected_data: json!({ "docID": m::r("john") }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { _commits(docID: ""#),
                    Text::reference("john"),
                    Text::from(r#"" order: {height: ASC}) { fieldName } }"#),
                ]),
                expected_data: json!({
                    "_commits": [{ "fieldName": "name" }, { "fieldName": "_C" }],
                }),
                ..Default::default()
            }
            .into(),
            names(json!([{ "name": "Jane" }, { "name": "John" }])).into(),
        ],
        ..Default::default()
    }
    .execute();
}

/// Pulling an updated John from a peer restores him whole on the node that
/// truncated him, rather than leaving a partial document.
#[test]
#[mantra_macros::req_test("defra.collection.truncate.filter_branchable")]
fn a_peer_update_restores_a_document_truncated_from_a_branchable_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            network_node("db0").into(),
            network_node("db1").into(),
            network_users("db0").into(),
            network_users("db1").into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
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
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 1 })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("john", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("john"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("createHead", "/cids/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("john")),
                cid: Some(Text::reference("createHead")),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            truncate_named_on(Some("db0"), "John").into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("john"),
                doc_map: json!({ "age": 2 }),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("john")),
                updated_fields: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncDocs {
                collection: "Users".into(),
                doc_ids: vec![Text::reference("john")],
                concurrency: 1,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name age } }".into(),
                expected_data: json!({ "Users": [{ "name": "John", "age": 2 }] }),
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

/// A peer that receives the whole collection after the truncate sees John
/// again, because the kept blocks are part of the collection DAG it syncs.
#[test]
#[mantra_macros::req_test("defra.collection.truncate.filter_branchable")]
fn a_fresh_peer_sync_restores_a_document_truncated_from_a_branchable_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            network_node("db0").into(),
            network_node("db1").into(),
            network_users("db0").into(),
            network_users("db1").into(),
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 1 })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            truncate_named_on(Some("db0"), "John").into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["Users".into()],
                collection_index: 0,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name age } }".into(),
                expected_data: json!({ "Users": [{ "name": "John", "age": 1 }] }),
                common: Common {
                    node: Some("db1".into()),
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

fn network_node(name: &str) -> Actions {
    NewNode {
        network: true,
        common: Common {
            node: Some(name.into()),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn network_users(node: &str) -> Actions {
    AddCollection {
        sdl: "type Users @branchable { name: String age: Int }".into(),
        common: Common {
            node: Some(node.into()),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}
