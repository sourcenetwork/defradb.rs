//! Client transaction isolation - a unique value raced by two writers.
//!
//! Drives the binary `support::release_binary()` picks through its CLI:
//! `target/debug/defradb`, unless `DEFRA_CONFORMANCE_BINARY` names another
//! artifact. As `ClientUniqueAdmission.tla` names the race: each writer reads the value as absent with a filter, each
//! adds it, and the guarantee in `docs/book/src/txn/guarantees.md` decides the
//! outcome. With a unique index exactly one commits and the other is refused
//! and leaves nothing; without one, both commit. The documents differ in
//! `age`, so their DocIDs differ and the race is decided on the unique entry.
//!
//! The autocommit race is driven in sequence: the loser's create is the re-run
//! the node makes after losing, which meets the winner's entry. The CLI prints
//! no error code, so the refusal is checked by its text.

use crate::support;
use defra_harness::{DefraClient, TestCluster};
use serde_json::Value;

const UNIQUE: &str = "type User { name: String @index(unique: true) age: Int }";
const PLAIN: &str = "type User { name: String age: Int }";
const VIOLATION: &str = "can not index a doc's field(s) that violates unique index";
const FIND_ALICE: &str = r#"query { User(filter: {name: {_eq: "alice"}}) { age } }"#;
const USERS: &str = "query { User(order: {age: ASC}) { name age } }";

fn add(age: u8) -> String {
    format!(r#"mutation {{ add_User(input: {{name: "alice", age: {age}}}) {{ _docID }} }}"#)
}

/// The refusal text of a request, whether the CLI failed or answered with
/// GraphQL errors; `None` when the request succeeded.
fn refusal<E>(result: Result<Value, E>) -> Option<String>
where
    E: std::fmt::Debug,
{
    match result {
        Err(error) => Some(format!("{error:?}")),
        Ok(value) => value.get("errors").map(Value::to_string),
    }
}

fn ages(node: &DefraClient) -> Vec<Value> {
    let users = node.query(USERS).expect("list users");
    users["User"]
        .as_array()
        .expect("User list")
        .iter()
        .map(|user| user["age"].clone())
        .collect()
}

/// Two explicit transactions read alice as absent, both add her, and the
/// first commit wins; returns the second commit's result.
async fn explicit_race(sdl: &str) -> (TestCluster, Result<String, String>) {
    let cluster = TestCluster::builder()
        .rust_nodes(1)
        .with_rust_binary(support::release_binary())
        .build()
        .await
        .expect("build single-node cluster");
    let node = cluster.client(0);
    node.schema_add(sdl).expect("add User schema");
    let tx1 = node.tx_create().expect("tx1");
    let tx2 = node.tx_create().expect("tx2");
    for tx in [&tx1, &tx2] {
        let read = node.query_with_tx(FIND_ALICE, tx).expect("filtered read");
        assert_eq!(read["User"], serde_json::json!([]), "alice reads as absent");
    }
    node.query_with_tx(&add(1), &tx1).expect("tx1 adds alice");
    node.query_with_tx(&add(2), &tx2).expect("tx2 adds alice");
    node.tx_commit(&tx1).expect("the first commit wins");
    let second = node.tx_commit(&tx2).map_err(|e| format!("{e:?}"));
    (cluster, second)
}

#[nuclei::test]
async fn explicit_loser_is_refused_and_leaves_nothing() {
    edge::compat(async move {
        let (cluster, second) = explicit_race(UNIQUE).await;
        let node = cluster.client(0);
        let error = second.expect_err("INV_AtMostOnePerNonNilValue: the second commit is refused");
        assert!(
            error.contains("transaction conflict"),
            "INV_LoserRefusedTyped: the explicit loser is told of the conflict, got {error}"
        );
        assert_eq!(
            ages(&node),
            [1],
            "INV_LoserLeavesNothing: only the winner is stored"
        );
    })
    .await
}

#[nuclei::test]
async fn autocommit_loser_is_refused_as_a_unique_violation() {
    edge::compat(async move {
        let cluster = TestCluster::builder()
            .rust_nodes(1)
            .with_rust_binary(support::release_binary())
            .build()
            .await
            .expect("build single-node cluster");
        let node = cluster.client(0);
        node.schema_add(UNIQUE).expect("add User schema");
        for _ in 0..2 {
            let read = node.query(FIND_ALICE).expect("filtered read");
            assert_eq!(read["User"], serde_json::json!([]), "alice reads as absent");
        }
        assert_eq!(
            refusal(node.query(&add(1))),
            None,
            "the first create commits"
        );
        let error = refusal(node.query(&add(2))).expect("the second create is refused");
        assert!(
            error.contains(VIOLATION) && !error.contains("transaction conflict"),
            "INV_LoserRefusedTyped: the autocommit loser is a unique violation, got {error}"
        );
        assert_eq!(
            ages(&node),
            [1],
            "INV_LoserLeavesNothing: only the winner is stored"
        );
    })
    .await
}

#[nuclei::test]
async fn without_an_index_both_commit() {
    edge::compat(async move {
        let (cluster, second) = explicit_race(PLAIN).await;
        let node = cluster.client(0);
        second.expect("INV_NoIndexNotRefused: without a unique index the second commit stands");
        assert_eq!(ages(&node), [1, 2], "both writers' documents are stored");
    })
    .await
}
