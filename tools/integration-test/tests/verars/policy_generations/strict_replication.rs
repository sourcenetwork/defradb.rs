//! Replicator trust and owner-authenticated HTTP writes do not authorize the
//! serving node's signature to mutate a shared native ACP document.

use integration_test::{generate_identity, users_schema_with_policy, DefraClient, USER_ACP_POLICY};
use serde_json::Value;
use std::time::Duration;

use super::{commit_ids, helpers, replication};

const DEADLINE: Duration = Duration::from_secs(30);

enum RelayGrant {
    Reader,
    Writer,
    RevokedWriter,
}

fn address(node: &DefraClient) -> String {
    node.p2p_info().expect("P2P addresses")[0]
        .as_str()
        .expect("P2P address")
        .to_owned()
}

fn document(node: &DefraClient, id: &str, owner: &str) -> Value {
    node.query_with_identity(
        &format!(r#"query {{ User(docID: "{id}") {{ _docID name age }} }}"#),
        owner,
    )
    .expect("owner reads protected document")
}

fn anonymous_stays_denied(node: &DefraClient, id: &str) {
    let value = node
        .query(&format!(r#"query {{ User(docID: "{id}") {{ _docID }} }}"#))
        .expect("anonymous document query");
    assert!(value["User"].as_array().unwrap().is_empty());
    assert!(commit_ids(node, id, None).is_empty());
}

async fn exercise(delete: bool, grant: RelayGrant) {
    let owner = helpers::funded_identity();
    let relay = generate_identity(&helpers::defra_binary()).expect("distinct relay identity");
    assert_ne!(owner.did, relay.did);
    let hub = helpers::start_hub_cluster().await;
    let cluster = helpers::vera_rs_builder(&hub.node(0).rpc_url(), &owner.private_key_hex, 2, true)
        .with_node_identity(1, relay.private_key_hex.clone())
        .with_signing()
        .build()
        .await
        .expect("native ACP strict replication cluster");
    let nodes: Vec<_> = (0..2).map(|index| cluster.client(index)).collect();
    let origin = &nodes[0];
    let other = &nodes[1];
    assert_eq!(origin.node_identity().unwrap()["DID"], owner.did);
    assert_eq!(other.node_identity().unwrap()["DID"], relay.did);
    let policy = origin
        .acp_policy_add(USER_ACP_POLICY, &owner.private_key_hex)
        .expect("create shared native policy");
    let policy_id = policy["PolicyID"]
        .as_str()
        .or_else(|| policy["policyID"].as_str())
        .expect("policy ID");
    for node in &nodes {
        node.schema_add_with_identity(&users_schema_with_policy(policy_id), &owner.private_key_hex)
            .expect("attach shared native policy");
    }
    let created = origin
        .query_with_identity(
            r#"mutation { add_User(input: {name: "Alice", age: 20}) { _docID } }"#,
            &owner.private_key_hex,
        )
        .expect("create protected document");
    let id = created["add_User"][0]["_docID"].as_str().unwrap();
    origin
        .acp_relationship_add("User", id, "reader", &relay.did, &owner.private_key_hex)
        .expect("grant relay read, independently of update/delete");
    if matches!(grant, RelayGrant::Writer | RelayGrant::RevokedWriter) {
        origin
            .acp_relationship_add("User", id, "writer", &relay.did, &owner.private_key_hex)
            .expect("grant relay writer");
    }
    replication::wait_for_submission(&cluster, 0).await;
    let original_commits = commit_ids(origin, id, Some(&owner.private_key_hex));
    assert!(!original_commits.is_empty());
    let original_document = document(origin, id, &owner.private_key_hex);
    replication::connect(&nodes, &owner.private_key_hex);
    replication::wait_for_document(other, id, &owner.private_key_hex, &original_commits).await;
    assert_eq!(
        document(other, id, &owner.private_key_hex),
        original_document
    );

    if matches!(grant, RelayGrant::RevokedWriter) {
        origin
            .acp_relationship_delete("User", id, "writer", &relay.did, &owner.private_key_hex)
            .expect("revoke relay writer while retaining read");
        replication::wait_for_submission(&cluster, 0).await;
    }
    // The relay authorizes its own outgoing transport; that does not grant its
    // composite signature update/delete permission at the receiving node.
    other
        .p2p_replicator_set_with_identity(&["User"], &address(origin), &relay.private_key_hex)
        .expect("authorize reverse replication as the relay");
    let (mutation, field, permission) = if delete {
        (
            format!(r#"mutation {{ delete_User(docID: "{id}") {{ _docID }} }}"#),
            "delete_User",
            "delete",
        )
    } else {
        (
            format!(
                r#"mutation {{ update_User(docID: "{id}", input: {{age: 21}}) {{ _docID }} }}"#
            ),
            "update_User",
            "update",
        )
    };
    let changed = other
        .query_with_identity(&mutation, &owner.private_key_hex)
        .expect("owner edits on the distinct signing relay");
    assert_eq!(changed[field][0]["_docID"], id);
    let composites = other
        .query_with_identity(
            &format!(r#"query {{ _commits(docID: "{id}", filter: {{fieldName: {{_eq: "_C"}}}}, order: {{height: DESC}}) {{ cid height signature {{ type }} }} }}"#),
            &owner.private_key_hex,
        )
        .expect("read relay composite history");
    assert_eq!(
        composites["_commits"][0]["height"], 2,
        "must identify the edit"
    );
    assert!(
        composites["_commits"][0]["signature"].is_object(),
        "the relay edit must carry a block signature before testing signer authorization"
    );
    let head = composites["_commits"][0]["cid"].as_str().unwrap();
    assert!(!original_commits.iter().any(|cid| cid == head));
    let edited_commits = commit_ids(other, id, Some(&owner.private_key_hex));
    assert!(edited_commits.iter().any(|cid| cid == head));
    let edited_document = document(other, id, &owner.private_key_hex);
    if delete {
        assert!(edited_document["User"].as_array().unwrap().is_empty());
    } else {
        assert_eq!(edited_document["User"][0]["age"], 21);
    }

    if matches!(grant, RelayGrant::Writer) {
        tokio::time::timeout(DEADLINE, async {
            loop {
                if document(origin, id, &owner.private_key_hex) == edited_document
                    && commit_ids(origin, id, Some(&owner.private_key_hex)) == edited_commits
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("explicitly granted relay edit must converge with its exact commit history");
    } else {
        let log = cluster.nodes[0]
            .rootdir
            .parent()
            .unwrap()
            .join("logs/stdout.log");
        let reason = format!("signer {} lacks {permission} permission", relay.did);
        tokio::time::timeout(DEADLINE, async {
            loop {
                if std::fs::read_to_string(&log).is_ok_and(|text| {
                    text.lines().any(|line| {
                        line.contains("quarantined")
                            && line.contains(head)
                            && line.contains(&reason)
                    })
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("exact signed edit must be quarantined for the missing write permission");
        assert_eq!(
            document(origin, id, &owner.private_key_hex),
            original_document
        );
        assert_eq!(
            commit_ids(origin, id, Some(&owner.private_key_hex)),
            original_commits
        );
    }
    for node in &nodes {
        anonymous_stays_denied(node, id);
    }
}

#[tokio::test]
#[serial_test::serial]
async fn native_replicated_update_rejects_reader_only_signer() {
    exercise(false, RelayGrant::Reader).await;
}

#[tokio::test]
#[serial_test::serial]
async fn native_replicated_delete_rejects_revoked_writer() {
    exercise(true, RelayGrant::RevokedWriter).await;
}

#[tokio::test]
#[serial_test::serial]
async fn native_replicated_update_accepts_granted_writer() {
    exercise(false, RelayGrant::Writer).await;
}

#[tokio::test]
#[serial_test::serial]
async fn native_replicated_delete_accepts_granted_writer() {
    exercise(true, RelayGrant::Writer).await;
}
