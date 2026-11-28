//! A collection patch inside a transaction is seen by that transaction alone
//! until it commits, leaves nothing behind when the transaction is discarded
//! or the patch fails, and conflicts with a patch committed first, through
//! every client.

use integration_test::actions::{
    AddCollection, AddDoc, AddNACActorRelationship, Close, DeleteNACActorRelationship,
    PatchCollection, Request, Start, TxnBegin, TxnCommit, TxnDiscard,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

const ADD_EMAIL: &str = r#"
    [
        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} }
    ]
"#;

const ADD_AGE: &str = r#"
    [
        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "age", "Kind": "Int"} }
    ]
"#;

const NO_EMAIL: &str = r#"Cannot query field "email" on type "Users"."#;

fn users() -> AddCollection {
    AddCollection {
        sdl: r#"
            type Users {
                name: String
            }
        "#
        .into(),
        ..Default::default()
    }
}

fn users_by(identity: u64) -> AddCollection {
    let mut users = users();
    users.common.identity_seed = Some(identity);
    users
}

fn query(txn: Option<usize>, fields: &str, expected: Result<serde_json::Value, &str>) -> Request {
    let (expected_data, expected_error) = match expected {
        Ok(data) => (data, None),
        Err(error) => (serde_json::Value::Null, Some(error.into())),
    };
    Request {
        txn,
        request: format!("query {{ Users {{ {fields} }} }}").into(),
        expected_data,
        expected_error,
        ..Default::default()
    }
}

fn patch(txn: Option<usize>, patch: &str) -> PatchCollection {
    PatchCollection {
        txn,
        patch: patch.into(),
        ..Default::default()
    }
}

fn patch_by(txn: Option<usize>, operations: &str, identity: u64) -> PatchCollection {
    let mut patched = patch(txn, operations);
    patched.common.identity_seed = Some(identity);
    patched
}

#[test]
#[mantra_macros::req_test("defra.txn.definitions.patch")]
fn txn_patch_collection_is_seen_by_its_txn_alone_until_commit() {
    TestCase::from_actions(vec![
        users().into(),
        TxnBegin::default().into(),
        patch(Some(0), ADD_EMAIL).into(),
        AddDoc {
            txn: Some(0),
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "John", "email": "john@example.com" })),
            ..Default::default()
        }
        .into(),
        query(
            Some(0),
            "name email",
            Ok(json!({ "Users": [{ "name": "John", "email": "john@example.com" }] })),
        )
        .into(),
        query(None, "name email", Err(NO_EMAIL)).into(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        query(
            None,
            "name email",
            Ok(json!({ "Users": [{ "name": "John", "email": "john@example.com" }] })),
        )
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.definitions.patch")]
fn txn_patch_collection_discarded_leaves_no_trace() {
    TestCase::from_actions(vec![
        users().into(),
        TxnBegin::default().into(),
        patch(Some(0), ADD_EMAIL).into(),
        query(Some(0), "name email", Ok(json!({ "Users": [] }))).into(),
        TxnDiscard {
            txn: 0,
            ..Default::default()
        }
        .into(),
        query(None, "name email", Err(NO_EMAIL)).into(),
        patch(None, ADD_AGE).into(),
        query(None, "name age", Ok(json!({ "Users": [] }))).into(),
        query(None, "name email", Err(NO_EMAIL)).into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.definitions.patch")]
fn txn_patch_collection_that_fails_leaves_its_txn_unchanged() {
    TestCase::from_actions(vec![
        users().into(),
        TxnBegin::default().into(),
        PatchCollection {
            txn: Some(0),
            patch: r#"
                [
                    { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} },
                    { "op": "replace", "path": "/Users/Name", "value": "" }
                ]
            "#
            .into(),
            expected_error: Some("collection name can't be empty".into()),
            ..Default::default()
        }
        .into(),
        query(Some(0), "name email", Err(NO_EMAIL)).into(),
        patch(Some(0), ADD_AGE).into(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        query(None, "name age", Ok(json!({ "Users": [] }))).into(),
        query(None, "name email", Err(NO_EMAIL)).into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.definitions.patch")]
fn txn_patch_collection_that_fails_after_writing_leaves_its_txn_unchanged() {
    TestCase::from_actions(vec![
        users().into(),
        AddCollection {
            sdl: "type Books { title: String }".into(),
            ..Default::default()
        }
        .into(),
        TxnBegin::default().into(),
        PatchCollection {
            txn: Some(0),
            patch: r#"
                [
                    { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} },
                    { "op": "add", "path": "/Books/Fields/-", "value": {"Name": "author", "Kind": "Users"} }
                ]
            "#
            .into(),
            expected_error: Some("relation name cannot be empty. Field: author".into()),
            ..Default::default()
        }
        .into(),
        query(Some(0), "name email", Err(NO_EMAIL)).into(),
        patch(Some(0), ADD_AGE).into(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        query(None, "name age", Ok(json!({ "Users": [] }))).into(),
        Request {
            request: "query { Books { title } }".into(),
            expected_data: json!({ "Books": [] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.definitions.patch")]
fn txn_patch_collection_conflicts_with_a_patch_committed_first() {
    TestCase::from_actions(vec![
        users().into(),
        TxnBegin::default().into(),
        TxnBegin::default().into(),
        patch(Some(0), ADD_EMAIL).into(),
        patch(Some(1), ADD_AGE).into(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 1,
            expected_error: Some("transaction conflict. Please retry".into()),
            ..Default::default()
        }
        .into(),
        query(None, "name email", Ok(json!({ "Users": [] }))).into(),
        query(
            None,
            "name age",
            Err(r#"Cannot query field "age" on type "Users"."#),
        )
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.definitions.patch")]
fn txn_patch_collection_conflicts_with_another_definition_committed_first() {
    TestCase::from_actions(vec![
        users().into(),
        TxnBegin::default().into(),
        TxnBegin::default().into(),
        patch(Some(0), ADD_EMAIL).into(),
        AddCollection {
            txn: Some(1),
            sdl: "type Books { title: String }".into(),
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 1,
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 0,
            expected_error: Some("transaction conflict. Please retry".into()),
            ..Default::default()
        }
        .into(),
        query(None, "name email", Err(NO_EMAIL)).into(),
        Request {
            request: "query { Books { title } }".into(),
            expected_data: json!({ "Books": [] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.definitions.patch")]
fn txn_patch_collection_without_nac_permission_is_refused() {
    TestCase::from_actions(vec![
        Close {
            keep_data: true,
            ..Default::default()
        }
        .into(),
        Start {
            enable_nac: true,
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        users_by(1).into(),
        TxnBegin {
            common: Common {
                identity_seed: Some(2),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        PatchCollection {
            txn: Some(0),
            expected_error: Some(
                "not authorized to perform operation. Permission: patch-collection".into(),
            ),
            ..patch_by(Some(0), ADD_EMAIL, 2)
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.definitions.patch")]
fn txn_patch_collection_after_permission_revocation_conflicts() {
    TestCase::from_actions(vec![
        Close {
            keep_data: true,
            ..Default::default()
        }
        .into(),
        Start {
            enable_nac: true,
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        users_by(1).into(),
        AddNACActorRelationship {
            relation: "patch-collection".into(),
            target_identity_seed: Some(2),
            expected_data: json!({ "existedAlready": false }),
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        TxnBegin {
            common: Common {
                identity_seed: Some(2),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        DeleteNACActorRelationship {
            relation: "patch-collection".into(),
            target_identity_seed: Some(2),
            expected_data: json!({ "recordFound": true }),
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        patch_by(Some(0), ADD_EMAIL, 2).into(),
        TxnCommit {
            txn: 0,
            expected_error: Some("transaction conflict. Please retry".into()),
            common: Common {
                identity_seed: Some(2),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Users { name email } }".into(),
            expected_error: Some(NO_EMAIL.into()),
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
