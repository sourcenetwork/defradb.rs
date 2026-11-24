//! Encryption ACP key-distribution tests against a vera.rs deployment.
//!
//! Ported from Go: tests/integration/encryption/peer_acp_test.go
//!
//! These exercise the KMS dual-gate (PR #4778): a cross-peer DEK fetch is
//! served only if the *requesting node's* DID has DAC `read` on the document,
//! evaluated against a Vera policy. The user-facing query is
//! additionally gated by the querying identity's DAC permission.
//!
//! Run with:
//!   VERAD_BINARY=... cargo nextest run -p integration-test --test verars -- encryption_acp::
//!
//! Each test starts a vera.rs validator and 2-3 DefraDB nodes, so they are slow.

use std::time::Duration;

use integration_test::{extract_p2p_addr, generate_identity, TestCluster};

use super::helpers;

/// DAC policy mirroring the Go original (peer_acp_test.go). `owner` is
/// auto-injected by the system; the `read` permission is satisfied by the
/// `reader` relation, which both the user and a peer node can be granted.
const POLICY: &str = r#"name: Test Policy
description: A Policy

resources:
  - name: users
    permissions:
      - name: read
        expr: reader + updater + deleter
      - name: update
        expr: updater
      - name: delete
        expr: deleter
      - name: nothing
        expr: dummy
    relations:
      - name: reader
        types:
          - actor
      - name: updater
        types:
          - actor
      - name: deleter
        types:
          - actor
      - name: admin
        manages:
          - reader
        types:
          - actor
      - name: dummy
        types:
          - actor"#;

fn users_schema(policy_id: &str) -> String {
    format!(
        r#"type Users @policy(id: "{}", resource: "users") {{ name: String  age: Int }}"#,
        policy_id
    )
}

fn policy_id_of(v: &serde_json::Value) -> String {
    v["PolicyID"]
        .as_str()
        .or_else(|| v["policyID"].as_str())
        .expect("PolicyID")
        .to_string()
}

/// Poll node `idx` (as `identity`) until the `Users` query returns exactly
/// `expected` names, or panic after the deadline. Returns the final names.
async fn wait_for_names(
    cluster: &TestCluster,
    idx: usize,
    identity: &str,
    expected: &[&str],
    deadline: Duration,
) -> Vec<String> {
    let end = std::time::Instant::now() + deadline;
    loop {
        let result = cluster
            .client(idx)
            .query_with_identity("query { Users { name } }", identity)
            .expect("query");
        let mut last: Vec<String> = result["Users"]
            .as_array()
            .expect("Users array")
            .iter()
            .filter_map(|u| u["name"].as_str().map(str::to_string))
            .collect();
        last.sort();
        let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        want.sort();
        if last == want {
            return last;
        }
        if std::time::Instant::now() >= end {
            return last;
        }
        nuclei::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Build a 2-node Vera + encryption + libp2p cluster, deploy `POLICY`
/// + the Users schema on both nodes, and return `(cluster, policy_id, ids)`.
///
/// This deliberately does NOT wire node0 -> node1 replication. Callers connect
/// node1 explicitly via [`connect_and_replicate_without_delegation`] when they
/// want the doc to start syncing. This lets the positive test grant on-chain
/// `reader` relations BEFORE node1 ever sees the encrypted block, mirroring
/// Go's grant-then-`WaitForSync` ordering (peer_acp_test.go).
///
/// `node0` is the owner; `node1` runs under its own distinct DID so granting *that* DID `reader` is
/// meaningful for the KMS node-gate.
async fn setup_two_nodes() -> (TestCluster, String, NodeIds) {
    let binary = helpers::defra_binary();
    let node0 = helpers::funded_identity();
    let node1 = generate_identity(&binary).expect("node1 identity");

    let hub = helpers::start_hub_cluster().await;
    let cluster = helpers::vera_rs_builder(&hub.node(0).rpc_url(), &node0.private_key_hex, 2, true)
        // node1 runs under its own DID so the KMS node-gate is distinct.
        .with_node_identity(1, &node1.private_key_hex)
        .with_encryption()
        .build()
        .await
        .expect("build vera encryption cluster");

    let c0 = cluster.client(0);
    let c1 = cluster.client(1);

    let policy_id = policy_id_of(
        &c0.acp_policy_add(POLICY, &node0.private_key_hex)
            .expect("add policy on node0"),
    );

    let schema = users_schema(&policy_id);
    c0.schema_add_with_identity(&schema, &node0.private_key_hex)
        .expect("schema on node0");

    // node1 deploys the same schema: it reads the policy node0 created from Vera.
    c1.schema_add_with_identity(&schema, &node0.private_key_hex)
        .expect("schema on node1");

    // Collections are registered for P2P on both nodes here; the actual
    // connection + replicator (which starts the push of the encrypted DAG to
    // node1) is deferred to `connect_and_replicate_without_delegation` so
    // callers control timing.
    c0.p2p_collection_add(&["Users"]).expect("col add node0");
    c1.p2p_collection_add(&["Users"]).expect("col add node1");

    (
        cluster,
        policy_id,
        NodeIds {
            _hub: hub,
            node0_key: node0.private_key_hex,
            node1_key: node1.private_key_hex,
            node1_did: node1.did,
        },
    )
}

struct NodeIds {
    _hub: vera_harness::cluster::TestCluster,
    node0_key: String,
    node1_key: String,
    node1_did: String,
}

/// Connect node0 -> node1 without an explicit-replay delegation.
///
/// An identity-bearing replicator request intentionally delegates the
/// authorizer's collection access to the target peer, including its KMS fetch.
/// Tests that assert the target node remains unauthorized must therefore use
/// the ordinary node-authenticated replication path.
fn connect_and_replicate_without_delegation(cluster: &TestCluster) {
    let addr1 = extract_p2p_addr(cluster, 1);
    let c0 = cluster.client(0);
    c0.p2p_connect(&[&addr1]).expect("connect");
    c0.p2p_replicator_set(&["Users"], &addr1)
        .expect("set replicator");
}

/// Port: TestDocEncryptionACP_IfUserAndNodeHaveAccess_ShouldFetch
///
/// node0 creates an encrypted doc and grants `reader` to BOTH a user DID
/// AND node1's DID. The user, querying on node1, sees the decrypted doc:
/// the user-gate (query ACP) and the node-gate (KMS key fetch) both pass.
#[nuclei::test]
#[serial_test::serial]
async fn encryption_acp_user_and_node_access() {
    edge::compat(async {
        let (cluster, _policy_id, ids) = setup_two_nodes().await;

        let binary = integration_test::rust_binary();
        let user = generate_identity(&binary).expect("user identity");

        let c0 = cluster.client(0);

        // Encrypted doc owned by node0.
        let created = c0
            .query_with_identity(
                r#"mutation { add_Users(input: {name: "Fred", age: 33}, encrypt: true) { _docID } }"#,
                &ids.node0_key,
            )
            .expect("create encrypted doc");
        let doc_id = created["add_Users"][0]["_docID"]
            .as_str()
            .expect("_docID")
            .to_string();

        // Grant reader to the user (user-gate) and to node1's DID (node-gate)
        // BEFORE node1 is connected. `acp_relationship_add` shells out to the CLI,
        // which submits the Vera grant tx and returns once it is committed, so
        // both grants are durable on-chain by the time we connect node1 below.
        // This mirrors Go's grant-then-`WaitForSync` ordering (peer_acp_test.go):
        // node1 only ever sees the encrypted block AFTER its `reader` grant exists,
        // so node0's KMS serve-gate authorizes the DEK fetch on the first attempt
        // (the Rust receiving node decrypts eagerly-once at merge and has no
        // late-grant retry, so the grant MUST precede the block).
        c0.acp_relationship_add("Users", &doc_id, "reader", &user.did, &ids.node0_key)
            .expect("grant user reader");
        c0.acp_relationship_add("Users", &doc_id, "reader", &ids.node1_did, &ids.node0_key)
            .expect("grant node1 reader");

        // Only now wire replication so the doc syncs to node1 with grants in place.
        connect_and_replicate_without_delegation(&cluster);

        // The user on node1 should eventually see the decrypted doc.
        let names = wait_for_names(
            &cluster,
            1,
            &user.private_key_hex,
            &["Fred"],
            Duration::from_secs(40),
        )
        .await;
        assert_eq!(
            names,
            vec!["Fred".to_string()],
            "user on node1 should see Fred"
        );
    })
    .await;
}

/// Port: TestDocEncryptionACP_IfUserHasAccessButNotNode_ShouldNotFetch
///
/// Grant `reader` to the user but NOT to node1's DID. node1's KMS cannot fetch
/// the DEK (node-gate denies), so the user — even with a query-gate grant —
/// sees nothing, and node1 has no commit blocks for the doc.
///
/// This now passes with the DEK-leak fix (#976): node0 registers the encrypted
/// doc on Vera *before* the detached P2P broadcast fires (the broadcast
/// mutator's pre-broadcast registration is now wired via
/// `crates/entry-points/cli/src/commands/start/server_p2p.rs`), so there is no
/// unregistered-window leak. With the doc registered and node1 never granted
/// `reader`, node0's KMS serve-gate denies the DEK fetch and node1 cannot
/// decrypt the replicated block. The grant-before-connect ordering below keeps
/// the on-chain state settled before node1 ever sees the doc.
#[nuclei::test]
#[serial_test::serial]
async fn encryption_acp_user_access_not_node() {
    edge::compat(async {
        let (cluster, _policy_id, ids) = setup_two_nodes().await;

        let binary = integration_test::rust_binary();
        let user = generate_identity(&binary).expect("user identity");

        let c0 = cluster.client(0);
        let c1 = cluster.client(1);

        let created = c0
            .query_with_identity(
                r#"mutation { add_Users(input: {name: "Fred", age: 33}, encrypt: true) { _docID } }"#,
                &ids.node0_key,
            )
            .expect("create encrypted doc");
        let doc_id = created["add_Users"][0]["_docID"]
            .as_str()
            .expect("_docID")
            .to_string();

        // Only the user gets reader; node1's DID does NOT. Grant before connecting
        // node1 so the on-chain state is settled when the block replicates.
        c0.acp_relationship_add("Users", &doc_id, "reader", &user.did, &ids.node0_key)
            .expect("grant user reader");

        // Now wire replication without an explicit-replay delegation. The target
        // node has no `reader` grant, so neither private replay admission nor a
        // KMS DEK fetch can borrow the owner's authority.
        connect_and_replicate_without_delegation(&cluster);

        // Give replication + (failed) key-fetch a chance to settle.
        nuclei::time::sleep(Duration::from_secs(8)).await;

        let result = c1
            .query_with_identity("query { Users { name } }", &user.private_key_hex)
            .expect("user query on node1");
        let users = result["Users"].as_array().expect("Users array");
        assert_eq!(
            users.len(),
            0,
            "user on node1 must see nothing when node lacks the key: {:?}",
            users
        );

        // Without node-level rights, node1 must not expose the document. Private
        // replay admission can reject it before storage; if it reaches storage,
        // the KMS gate must still prevent decryption. The empty result asserts the
        // end-to-end security property without depending on which gate rejects it.
    })
    .await;
}

/// Port: TestDocEncryptionACP_IfNodeHasAccessToSomeDocs_ShouldFetchOnlyThem
///
/// Six docs: encrypted/plaintext crossed with private-shared / private-unshared
/// / created without an identity. node1 is granted `reader` (as a node) only on
/// the two private docs it is meant to read. It ends up able to read exactly
/// those two (encrypted+shared, plaintext+shared): not the two private-unshared
/// docs, and not the two created without an identity, which Go's chain treats
/// as public and vera.rs, which denies an unregistered document, does not.
#[nuclei::test]
#[serial_test::serial]
async fn encryption_acp_node_partial_access() {
    edge::compat(async {
        let (cluster, _policy_id, ids) = setup_two_nodes().await;

        let c0 = cluster.client(0);

        // Helper: create a doc as node0, optionally encrypted, return its docID.
        let create = |name: &str, encrypted: bool| -> String {
            let mutation = if encrypted {
                format!(
                    r#"mutation {{ add_Users(input: {{name: "{}", age: 33}}, encrypt: true) {{ _docID }} }}"#,
                    name
                )
            } else {
                format!(
                    r#"mutation {{ add_Users(input: {{name: "{}", age: 33}}) {{ _docID }} }}"#,
                    name
                )
            };
            let created = c0
                .query_with_identity(&mutation, &ids.node0_key)
                .unwrap_or_else(|e| panic!("create {}: {:?}", name, e));
            created["add_Users"][0]["_docID"]
                .as_str()
                .unwrap_or_else(|| panic!("missing _docID for {}", name))
                .to_string()
        };

        // Docs created without an owner identity are unregistered.
        let unregistered_create = |name: &str, encrypted: bool| -> String {
            let mutation = if encrypted {
                format!(
                    r#"mutation {{ add_Users(input: {{name: "{}", age: 33}}, encrypt: true) {{ _docID }} }}"#,
                    name
                )
            } else {
                format!(
                    r#"mutation {{ add_Users(input: {{name: "{}", age: 33}}) {{ _docID }} }}"#,
                    name
                )
            };
            let created = c0
                .query(&mutation)
                .unwrap_or_else(|e| panic!("create unregistered {}: {:?}", name, e));
            created["add_Users"][0]["_docID"]
                .as_str()
                .unwrap_or_else(|| panic!("missing _docID for unregistered {}", name))
                .to_string()
        };

        // encrypted, private, shared -> Fred
        let fred = create("Fred", true);
        c0.acp_relationship_add("Users", &fred, "reader", &ids.node1_did, &ids.node0_key)
            .expect("share Fred to node1");

        // encrypted, private, NOT shared -> Andy
        let _andy = create("Andy", true);

        // encrypted, unregistered -> Islam
        let _islam = unregistered_create("Islam", true);

        // plaintext, private, shared -> John
        let john = create("John", false);
        c0.acp_relationship_add("Users", &john, "reader", &ids.node1_did, &ids.node0_key)
            .expect("share John to node1");

        // plaintext, private, NOT shared -> Keenan
        let _keenan = create("Keenan", false);

        // plaintext, unregistered -> Shahzad
        let _shahzad = unregistered_create("Shahzad", false);

        // The grants are finalized; wire replication so node1 starts syncing.
        connect_and_replicate_without_delegation(&cluster);

        // node1 queries as itself: it ends up with exactly the two docs it was
        // granted, and a doc that arrives later does not widen that.
        let entitled = ["Fred", "John"];
        let names = wait_for_names(
            &cluster,
            1,
            &ids.node1_key,
            &entitled,
            Duration::from_secs(50),
        )
        .await;
        assert_eq!(names, entitled, "node1 should see the two granted docs");
        nuclei::time::sleep(Duration::from_secs(8)).await;
        let settled = wait_for_names(&cluster, 1, &ids.node1_key, &entitled, Duration::ZERO).await;
        assert_eq!(
            settled, entitled,
            "node1 should see exactly the two granted docs"
        );
    })
    .await;
}
