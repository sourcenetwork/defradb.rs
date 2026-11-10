//! AddView and RefreshViews in a transaction need what they need outside
//! one (AddView only `add-view`, as in Go), and an embedded caller that names no identity acts as the node in a
//! transaction, as the other definition changes do.

use integration_test::actions::{
    Actions, AddCollection, AddNACActorRelationship, AddView, Close, RefreshViews, Start, TxnBegin,
    TxnCommit,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const QUERY: &str = r#"
    User {
        name
    }
"#;

const SDL: &str = r#"
    type UserView @materialized(if: false) {
        name: String
    }
"#;

const TXN_SDL: &str = r#"
    type UserTxnView @materialized(if: false) {
        name: String
    }
"#;

fn as_identity(seed: u64) -> Common {
    Common {
        identity_seed: Some(seed),
        ..Default::default()
    }
}

/// A NAC-enabled node owned by identity 1, holding a `User` collection.
fn nac_node() -> Vec<Actions> {
    vec![
        Close {
            keep_data: true,
            ..Default::default()
        }
        .into(),
        Start {
            enable_nac: true,
            node_identity_seed: Some(0),
            common: as_identity(1),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: r#"
                type User {
                    name: String
                }
            "#
            .into(),
            common: as_identity(1),
            ..Default::default()
        }
        .into(),
    ]
}

// Granting a single permission is a Rust NAC relation; Go's policy has
// only owner and admin, so the Go client is left out.
#[test]
#[mantra_macros::req_test("go.acp.nac.gates.views")]
fn txn_add_view_needs_only_add_view_as_outside_a_txn() {
    let mut actions = nac_node();
    let more: Vec<Actions> = vec![
        AddNACActorRelationship {
            relation: "add-view".into(),
            target_identity_seed: Some(2),
            expected_data: json!({ "existedAlready": false }),
            common: as_identity(1),
            ..Default::default()
        }
        .into(),
        AddView {
            query: QUERY.into(),
            sdl: SDL.into(),
            common: as_identity(2),
            ..Default::default()
        }
        .into(),
        TxnBegin {
            common: as_identity(2),
            ..Default::default()
        }
        .into(),
        AddView {
            txn: Some(0),
            query: QUERY.into(),
            sdl: TXN_SDL.into(),
            common: as_identity(2),
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 0,
            common: as_identity(2),
            ..Default::default()
        }
        .into(),
    ];
    actions.extend(more);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

// An embedded call that names no identity: only the in-process client makes
// one, and runs as the node in the node's transaction.
#[test]
#[mantra_macros::req_test("go.acp.nac.gates.views")]
fn txn_view_ops_without_identity_act_as_the_node() {
    let mut actions = nac_node();
    let more: Vec<Actions> = vec![
        TxnBegin {
            ..Default::default()
        }
        .into(),
        AddView {
            txn: Some(0),
            query: QUERY.into(),
            sdl: SDL.into(),
            ..Default::default()
        }
        .into(),
        RefreshViews {
            txn: Some(0),
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
    ];
    actions.extend(more);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}
