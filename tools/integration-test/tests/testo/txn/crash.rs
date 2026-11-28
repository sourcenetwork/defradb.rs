//! A node killed with SIGKILL during or right after a commit, then started
//! again on its directory, recovers each transaction whole.

use integration_test::actions::{
    Actions, AddCollection, CrashCommit, Kill, NewNode, Request, Restart, TxnBegin, TxnCommit,
};
use integration_test::crash::CrashPoint;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};
use testo::action::ActionInstance;

/// A node in its own process with a `Users` collection, and transaction 0
/// creating two users in it, not yet committed.
fn two_users_in_a_transaction() -> Vec<ActionInstance<Actions>> {
    vec![
        NewNode {
            own_process: true,
            database_dir: Some("".into()),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: r#"
                type Users {
                    name: String @index
                    age: Int
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        TxnBegin::default().into(),
        Request {
            txn: Some(0),
            request: r#"
                mutation {
                    add_Users(input: [{ name: "John", age: 27 }, { name: "Islam", age: 32 }]) {
                        name
                    }
                }
            "#
            .into(),
            expected_data: json!({
                "add_Users": integration_test::m::unordered(json!([
                    { "name": "John" },
                    { "name": "Islam" },
                ])),
            }),
            ..Default::default()
        }
        .into(),
    ]
}

/// Read the users back, through the collection and through the index on
/// `name`, expecting `users` both ways.
fn users_read_back(users: Value) -> Vec<ActionInstance<Actions>> {
    let index_fetches = users.as_array().map_or(0, Vec::len);
    vec![
        Request {
            request: r#"
                query {
                    Users {
                        name
                        age
                    }
                }
            "#
            .into(),
            expected_data: json!({ "Users": integration_test::m::unordered(users.clone()) }),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"
                query {
                    Users(filter: { name: { _in: ["John", "Islam", "Fred"] } }) {
                        name
                        age
                    }
                }
            "#
            .into(),
            expected_data: json!({ "Users": integration_test::m::unordered(users) }),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"
                query @explain(type: execute) {
                    Users(filter: { name: { _in: ["John", "Islam", "Fred"] } }) {
                        name
                        age
                    }
                }
            "#
            .into(),
            expected_data: json!(integration_test::m::explain_scan(
                json!({ "indexFetches": index_fetches })
            )),
            ..Default::default()
        }
        .into(),
    ]
}

fn both_users() -> Value {
    json!([
        { "name": "John", "age": 27 },
        { "name": "Islam", "age": 32 },
    ])
}

/// Kill the node at `point` in transaction 0's commit, start it again, and
/// read back `users`.
fn crash_commit_at(point: CrashPoint, users: Value) -> Vec<ActionInstance<Actions>> {
    let mut actions = two_users_in_a_transaction();
    actions.push(
        CrashCommit {
            txn: 0,
            point,
            ..Default::default()
        }
        .into(),
    );
    actions.push(Restart::default().into());
    actions.extend(users_read_back(users));
    actions
}

/// After a crash that left nothing, the node takes a new write, and keeps it
/// across a second kill.
fn new_write_survives_a_kill() -> Vec<ActionInstance<Actions>> {
    let mut actions = vec![
        Request {
            request: r#"
                mutation {
                    add_Users(input: [{ name: "Fred", age: 44 }]) {
                        name
                    }
                }
            "#
            .into(),
            expected_data: json!({ "add_Users": [{ "name": "Fred" }] }),
            ..Default::default()
        }
        .into(),
        Kill::default().into(),
        Restart::default().into(),
    ];
    actions.extend(users_read_back(json!([{ "name": "Fred", "age": 44 }])));
    actions
}

#[test]
#[mantra_macros::req_test("defra.txn.crash_recovery.committed")]
fn a_commit_answered_success_survives_a_kill() {
    let mut actions = two_users_in_a_transaction();
    actions.push(TxnCommit::default().into());
    actions.push(Kill::default().into());
    actions.push(Restart::default().into());
    actions.extend(users_read_back(both_users()));
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::RustHttp, ClientType::Cli, ClientType::GoHttp]),
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.crash_recovery.all_or_nothing")]
fn a_commit_killed_before_its_log_write_leaves_nothing() {
    let mut actions = crash_commit_at(CrashPoint::BeforeLogWrite, json!([]));
    actions.extend(new_write_survives_a_kill());
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::RustHttp, ClientType::Cli]),
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.crash_recovery.all_or_nothing")]
fn a_commit_killed_mid_log_write_leaves_nothing() {
    let mut actions = crash_commit_at(CrashPoint::TornLogWrite, json!([]));
    actions.extend(new_write_survives_a_kill());
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::RustHttp, ClientType::Cli]),
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.crash_recovery.all_or_nothing")]
fn a_commit_killed_after_its_log_sync_keeps_everything() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::RustHttp, ClientType::Cli]),
        actions: crash_commit_at(CrashPoint::AfterLogSync, both_users()),
        ..Default::default()
    }
    .execute();
}
