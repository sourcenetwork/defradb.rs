//! An in-process write signs with the acting client identity's own key, and
//! with the node's key when the request carries no identity, as Go's
//! `contextForSigning` does.

use integration_test::actions::shared::KeyType;
use integration_test::actions::Actions;
use integration_test::actions::{
    AddCollection, AddDoc, DeleteDoc, GetCollections, ImportBackup, NewNode, Request, UpdateDoc,
    VerifyBlockSignature, WaitForPeersEvents, WaitForSync,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;
use testo::action::ActionInstance;

const OTHER_KEY: &str = "signature was created by a different key";

fn verify(cid: &str, signer_seed: Option<u64>, error: Option<&str>) -> VerifyBlockSignature {
    VerifyBlockSignature {
        signer_seed,
        signer_node: signer_seed.is_none().then(|| "db".to_owned()),
        cid: Text::reference(cid),
        expected_error: error.map(str::to_owned),
        ..Default::default()
    }
}

fn head(capture: &'static str) -> WaitForSync {
    WaitForSync {
        doc_id: Some(Text::reference("doc")),
        common: Common {
            capture: captures([(capture, "/cid")]),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.signature.embedded_caller_key")]
fn signature_in_process_write_signs_with_the_callers_key() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Secp256k1),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String age: Int }".into(),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            head("created").into(),
            verify("created", Some(1), None).into(),
            verify("created", None, Some(OTHER_KEY)).into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "age": 22 }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            head("updated").into(),
            verify("updated", Some(1), None).into(),
            verify("updated", None, Some(OTHER_KEY)).into(),
        ],
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.signature.embedded_caller_key")]
fn signature_in_process_write_without_identity_signs_with_the_node_key() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Secp256k1),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String age: Int }".into(),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            head("created").into(),
            verify("created", None, None).into(),
            verify("created", Some(1), Some(OTHER_KEY)).into(),
        ],
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}

/// The newest-first `_commits` of the collection, the first `N` captured
/// under `names`.
fn commits<const N: usize>(names: [&str; N]) -> Request {
    let pointers: Vec<String> = (0..N).map(|i| format!("/data/_commits/{i}/cid")).collect();
    let mut capture = captures([]);
    for (name, pointer) in names.iter().zip(&pointers) {
        capture.insert((*name).to_owned(), pointer.clone());
    }
    Request {
        request: "query { _commits(order: {height: DESC}) { cid } }".into(),
        common: Common {
            capture,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.signature.embedded_caller_key")]
fn signature_in_process_field_and_delete_blocks_sign_with_the_callers_key() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Secp256k1),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String age: Int }".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Users(input: {name: "John", age: 21}) { _docID } }"#
                    .into(),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("doc", "/data/add_Users/0/_docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            commits(["first", "second", "third"]).into(),
            verify("first", Some(1), None).into(),
            verify("second", Some(1), None).into(),
            verify("third", Some(1), None).into(),
            verify("first", None, Some(OTHER_KEY)).into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            commits(["deleted"]).into(),
            verify("deleted", Some(1), None).into(),
            verify("deleted", None, Some(OTHER_KEY)).into(),
        ],
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}

/// A secp256k1-signing node with a `Users` collection, ready to sync.
fn signing_node(sdl: &str) -> Vec<ActionInstance<Actions>> {
    vec![
        NewNode {
            enable_signing: true,
            key_type: Some(KeyType::Secp256k1),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: sdl.into(),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("defra.signature.embedded_caller_key")]
fn signature_in_process_graphql_update_signs_with_the_callers_key() {
    let mut actions = signing_node("type Users { name: String age: Int }");
    actions.extend([
        Request {
            request: r#"mutation { add_Users(input: {name: "John", age: 21}) { _docID } }"#.into(),
            common: Common {
                identity_seed: Some(1),
                capture: captures([("doc", "/data/add_Users/0/_docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Request {
            request: Text::concat([
                Text::from(r#"mutation { update_Users(docID: ""#),
                Text::reference("doc"),
                Text::from(r#"", input: {age: 22}) { _docID } }"#),
            ]),
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query { _commits(order: {height: DESC}, filter: {fieldName: {_eq: "_C"}}) { cid } }"#.into(),
            common: Common {
                capture: captures([("updated", "/data/_commits/0/cid")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        verify("updated", Some(1), None).into(),
        verify("updated", None, Some(OTHER_KEY)).into(),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.signature.embedded_caller_key")]
fn signature_in_process_branchable_collection_block_signs_with_the_callers_key() {
    let mut actions = signing_node("type Users @branchable { name: String age: Int }");
    actions.extend([
        GetCollections {
            collection_name: Some("Users".into()),
            filtered: true,
            common: Common {
                capture: captures([("collection", "/collections/0/CollectionID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "age": 21, "name": "John" })),
            common: Common {
                identity_seed: Some(1),
                capture: captures([("doc", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        WaitForSync {
            doc_ids: Some(vec![Text::reference("doc"), Text::reference("collection")]),
            common: Common {
                capture: captures([("collectionHead", "/cids/1")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        verify("collectionHead", Some(1), None).into(),
        verify("collectionHead", None, Some(OTHER_KEY)).into(),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.signature.embedded_caller_key")]
fn signature_in_process_backup_import_signs_with_the_callers_key() {
    let mut actions = signing_node("type Users { name: String age: Int }");
    actions.extend([
        ImportBackup {
            import_content: r#"{"Users":[{"age":21,"name":"John"}]}"#.into(),
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        commits(["first", "second", "third"]).into(),
        verify("first", Some(1), None).into(),
        verify("second", Some(1), None).into(),
        verify("third", Some(1), None).into(),
        verify("first", None, Some(OTHER_KEY)).into(),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}
