//! Iroh P2P collection subscription management tests.
//!
//! Ported from Go: tests/integration/net/simple/peer/ (collection subscription tests)
//!
//! Run with:
//!   cargo test -p integration-test --test p2p_iroh replication::collection_sub

use std::time::Duration;

use integration_test::TestCluster;
use serial_test::serial;

const SCHEMA_USERS: &str = "type Users { name: String  age: Int }";
const SCHEMA_NOTES: &str = "type Notes { text: String  priority: Int }";
const P2P_TIMEOUT: Duration = Duration::from_secs(15);

async fn setup_iroh_node() -> TestCluster {
    let cluster = TestCluster::builder()
        .rust_nodes(1)
        .with_iroh_transport()
        .build()
        .await
        .unwrap();
    cluster
        .wait_for_log(0, "p2p_listening", P2P_TIMEOUT)
        .await
        .expect("P2P listener did not start");
    cluster
        .client(0)
        .schema_add(SCHEMA_USERS)
        .expect("schema Users");
    cluster
        .client(0)
        .schema_add(SCHEMA_NOTES)
        .expect("schema Notes");
    cluster
}

/// Port: TestP2PCollectionGetAll
/// Get all P2P collections when none configured returns empty.
#[nuclei::test]
#[serial]
async fn collection_get_all() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        let cols = node.p2p_collection_list().expect("p2p_collection_list");
        let arr = cols.as_array().expect("not array");
        assert!(arr.is_empty(), "should have 0 P2P collections initially");
    })
    .await;
}

/// Port: TestP2PCollectionAddGetSingle
/// Add and verify a single collection subscription.
#[nuclei::test]
#[serial]
async fn collection_add_get_single() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users"]).expect("add Users");

        let cols = node.p2p_collection_list().expect("list");
        let arr = cols.as_array().expect("not array");
        assert_eq!(arr.len(), 1, "should have 1 collection");
        assert_eq!(arr[0], "Users", "list should return the collection name");
    })
    .await;
}

#[nuclei::test]
#[serial]
async fn collection_subscription_persists_after_restart() {
    edge::compat(async move {
        let mut cluster = TestCluster::builder()
            .rust_nodes(1)
            .with_iroh_transport()
            .with_store("badger")
            .build()
            .await
            .unwrap();
        cluster
            .wait_for_log(0, "p2p_listening", P2P_TIMEOUT)
            .await
            .expect("P2P listener did not start");

        let node = cluster.client(0);
        node.schema_add(SCHEMA_USERS).expect("schema Users");
        node.p2p_collection_add(&["Users"]).expect("add Users");

        let before = node.p2p_collection_list().expect("list before restart");
        assert_eq!(
            before.as_array().expect("not array").len(),
            1,
            "collection should be subscribed before restart"
        );

        cluster
            .restart_node(0, Duration::from_secs(30))
            .await
            .expect("restart node");
        cluster
            .wait_for_log(0, "p2p_listening", P2P_TIMEOUT)
            .await
            .expect("P2P listener did not restart");

        let node_after_restart = cluster.client(0);
        let after = node_after_restart
            .p2p_collection_list()
            .expect("list after restart");
        assert_eq!(
            after,
            serde_json::json!(["Users"]),
            "collection name should persist across restart"
        );
    })
    .await;
}

/// Port: TestP2PCollectionAddGetMultiple
/// Add and verify multiple collection subscriptions.
#[nuclei::test]
#[serial]
async fn collection_add_get_multiple() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users"]).expect("add Users");
        node.p2p_collection_add(&["Notes"]).expect("add Notes");

        let cols = node.p2p_collection_list().expect("list");
        let arr = cols.as_array().expect("not array");
        assert_eq!(arr.len(), 2, "should have 2 collections");
    })
    .await;
}

/// Port: TestP2PCollectionAddRemoveGetSingle
/// Add, remove, verify single collection is gone.
#[nuclei::test]
#[serial]
async fn collection_add_remove_get_single() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users"]).expect("add");
        let before = node.p2p_collection_list().expect("list before");
        assert_eq!(before.as_array().expect("not array").len(), 1);

        node.p2p_collection_delete(&["Users"]).expect("remove");
        let after = node.p2p_collection_list().expect("list after");
        assert!(
            after.as_array().expect("not array").is_empty(),
            "should have 0 after remove"
        );
    })
    .await;
}

/// Port: TestP2PCollectionAddRemoveGetMultiple
/// Add two, remove one, verify one remains.
#[nuclei::test]
#[serial]
async fn collection_add_remove_get_multiple() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users"]).expect("add Users");
        node.p2p_collection_add(&["Notes"]).expect("add Notes");

        let before = node.p2p_collection_list().expect("list");
        assert_eq!(before.as_array().expect("not array").len(), 2);

        node.p2p_collection_delete(&["Users"])
            .expect("remove Users");

        let after = node.p2p_collection_list().expect("list after");
        let arr = after.as_array().expect("not array");
        assert_eq!(arr.len(), 1, "should have 1 after removing Users");
    })
    .await;
}

/// Port: TestP2PCollectionAddSingle
/// Simple add single collection succeeds.
#[nuclei::test]
#[serial]
async fn collection_add_single() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        let result = node.p2p_collection_add(&["Users"]);
        assert!(result.is_ok(), "adding single collection should succeed");

        let cols = node.p2p_collection_list().expect("list");
        assert_eq!(cols.as_array().expect("not array").len(), 1);
    })
    .await;
}

/// Port: TestP2PCollectionAddMultiple
/// Add multiple collections in one call succeeds.
#[nuclei::test]
#[serial]
async fn collection_add_multiple() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users", "Notes"])
            .expect("add both");

        let cols = node.p2p_collection_list().expect("list");
        let arr = cols.as_array().expect("not array");
        assert_eq!(arr.len(), 2, "should have 2 collections");
    })
    .await;
}

/// Port: TestP2PCollectionAddSingleErroneousCollectionID
/// Adding non-existent collection fails.
#[nuclei::test]
#[serial]
async fn collection_add_erroneous_id() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        let result = node.p2p_collection_add(&["NonExistent"]);
        assert!(
            result.is_err(),
            "adding non-existent collection should error"
        );
    })
    .await;
}

/// A mixed batch is validated before any subscription is committed.
#[nuclei::test]
#[serial]
async fn collection_add_mixed_batch_preserves_prior_subscription() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        let result = node.p2p_collection_add(&["Users", "NonExistent"]);
        assert!(result.is_err(), "mixed valid/invalid batch should error");

        let cols = node.p2p_collection_list().expect("list");
        let arr = cols.as_array().expect("not array");
        assert!(
            arr.is_empty(),
            "an invalid collection should reject the whole batch before commit"
        );
    })
    .await;
}

/// Port: TestP2PCollectionAddValidThenErroneousCollectionID
/// Add valid first, then try erroneous — valid should still exist.
#[nuclei::test]
#[serial]
async fn collection_add_valid_then_erroneous() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users"]).expect("add Users");
        let _ = node.p2p_collection_add(&["NonExistent"]); // may fail

        let cols = node.p2p_collection_list().expect("list");
        let arr = cols.as_array().expect("not array");
        assert!(!arr.is_empty(), "previously added Users should still exist");
    })
    .await;
}

/// Port: TestP2PCollectionAddNone
/// Adding empty list: behavior is implementation-defined.
#[nuclei::test]
#[serial]
async fn collection_add_none() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        let result = node.p2p_collection_add(&[]);
        // Empty add may succeed (no-op) or fail
        let cols = node.p2p_collection_list().expect("list");
        let arr = cols.as_array().expect("not array");
        assert!(
            arr.is_empty(),
            "no collections should exist after empty add"
        );
        drop(result);
    })
    .await;
}

/// Port: TestP2PCollectionAddAndRemoveSingle
/// Full lifecycle: add, verify, remove, verify empty.
#[nuclei::test]
#[serial]
async fn collection_add_and_remove_single() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users"]).expect("add");
        assert_eq!(
            node.p2p_collection_list()
                .expect("list")
                .as_array()
                .expect("not array")
                .len(),
            1
        );

        node.p2p_collection_delete(&["Users"]).expect("remove");
        assert!(
            node.p2p_collection_list()
                .expect("list")
                .as_array()
                .expect("not array")
                .is_empty(),
            "should be empty after remove"
        );
    })
    .await;
}

/// Port: TestP2PCollectionAddAndRemoveMultiple
/// Add multiple, remove all, verify empty.
#[nuclei::test]
#[serial]
async fn collection_add_and_remove_multiple() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users"]).expect("add Users");
        node.p2p_collection_add(&["Notes"]).expect("add Notes");
        assert_eq!(
            node.p2p_collection_list()
                .expect("list")
                .as_array()
                .expect("not array")
                .len(),
            2
        );

        node.p2p_collection_delete(&["Users"])
            .expect("remove Users");
        node.p2p_collection_delete(&["Notes"])
            .expect("remove Notes");
        assert!(
            node.p2p_collection_list()
                .expect("list")
                .as_array()
                .expect("not array")
                .is_empty(),
            "should be empty after removing all"
        );
    })
    .await;
}

/// Port: TestP2PCollectionAddSingleAndRemoveErroneous
/// Add valid, try to remove non-existent — valid should persist.
#[nuclei::test]
#[serial]
async fn collection_add_single_remove_erroneous() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users"]).expect("add Users");
        let _ = node.p2p_collection_delete(&["NonExistent"]); // may fail or no-op

        let cols = node.p2p_collection_list().expect("list");
        assert_eq!(
            cols.as_array().expect("not array").len(),
            1,
            "Users should persist after removing non-existent"
        );
    })
    .await;
}

/// Port: TestP2PCollectionAddSingleAndRemoveNone
/// Add valid, remove empty list — valid should persist.
#[nuclei::test]
#[serial]
async fn collection_add_single_remove_none() {
    edge::compat(async move {
        let cluster = setup_iroh_node().await;
        let node = cluster.client(0);

        node.p2p_collection_add(&["Users"]).expect("add Users");
        let _ = node.p2p_collection_delete(&[]); // empty remove

        let cols = node.p2p_collection_list().expect("list");
        assert_eq!(
            cols.as_array().expect("not array").len(),
            1,
            "Users should persist after empty remove"
        );
    })
    .await;
}
