//! HTTP authenticates the owner, but the serving node signs the resulting
//! blocks. Only an explicit write grant to that node permits inbound writes.

use super::*;

async fn owner_edit_on_relay(delete: bool, grant_writer: bool) {
    if !support::vera_binary_available() {
        eprintln!("skipping strict ACP write test: verad is not available");
        return;
    }
    let relay = generate_identity(&integration_test::rust_binary()).expect("relay identity");
    let (cluster, _, owner_key) = setup_vera_cluster_with_relay(Some(&relay.private_key_hex))
        .await
        .expect("Vera cluster");
    let origin = cluster.client(0);
    let other = cluster.client(1);
    let relay_identity = other.node_identity().expect("relay node identity");
    assert_eq!(relay_identity["DID"], relay.did);
    assert_ne!(
        origin.node_identity().expect("origin identity")["DID"],
        relay.did
    );

    let created = origin
        .query_with_identity(
            r#"mutation { add_User(input: {name: "Eve", age: 20}) { _docID } }"#,
            &owner_key,
        )
        .expect("create protected document");
    let doc_id = extract_doc_id(&created, "add_User");
    // Read is necessary for replication but deliberately insufficient to write.
    origin
        .acp_relationship_add("User", &doc_id, "reader", &relay.did, &owner_key)
        .expect("grant relay read");
    if grant_writer {
        origin
            .acp_relationship_add("User", &doc_id, "writer", &relay.did, &owner_key)
            .expect("grant relay update and delete");
    }
    setup_replicator(&cluster);
    let query = format!(r#"query {{ User(docID: "{doc_id}") {{ name age }} }}"#);
    poll_until(
        || {
            other
                .query_with_identity(&query, &owner_key)
                .is_ok_and(|data| data["User"][0]["age"] == 20)
        },
        P2P_TIMEOUT,
        Duration::from_millis(200),
        "genesis did not replicate",
    )
    .await;

    let addr0 = extract_p2p_addr(&cluster, 0);
    other
        .p2p_replicator_set(&["User"], &addr0)
        .expect("relay back to origin");
    let (mutation, field) = if delete {
        (
            format!(r#"mutation {{ delete_User(docID: "{doc_id}") {{ _docID }} }}"#),
            "delete_User",
        )
    } else {
        (
            format!(
                r#"mutation {{ update_User(docID: "{doc_id}", input: {{age: 21}}) {{ _docID }} }}"#
            ),
            "update_User",
        )
    };
    let changed = other
        .query_with_identity(&mutation, &owner_key)
        .expect("owner edits on relay");
    assert_eq!(extract_doc_id(&changed, field), doc_id);

    let commits = other.query_with_identity(
        &format!(r#"query {{ _commits(docID: "{doc_id}", filter: {{fieldName: {{_eq: "_C"}}}}, order: {{height: DESC}}) {{ cid height }} }}"#),
        &owner_key,
    ).expect("relay commits");
    let head = commits["_commits"][0]["cid"]
        .as_str()
        .expect("latest composite CID");
    assert_eq!(
        commits["_commits"][0]["height"], 2,
        "must observe the edit, not genesis"
    );

    if grant_writer {
        poll_until(
            || {
                origin
                    .query_with_identity(&query, &owner_key)
                    .is_ok_and(|data| {
                        if delete {
                            data["User"].as_array().is_some_and(Vec::is_empty)
                        } else {
                            data["User"][0]["age"] == 21
                        }
                    })
            },
            P2P_TIMEOUT,
            Duration::from_millis(200),
            "granted relay write did not replicate",
        )
        .await;
    } else {
        // A successful push only acknowledges durable intake. Wait for this
        // exact root's quarantine, rather than treating lack of delivery as denial.
        let log_path = cluster.nodes[0]
            .rootdir
            .parent()
            .expect("node directory")
            .join("logs/stdout.log");
        let permission = if delete { "delete" } else { "update" };
        let reason = format!("signer {} lacks {permission} permission", relay.did);
        poll_until(
            || {
                std::fs::read_to_string(&log_path).is_ok_and(|log| {
                    log.lines().any(|line| {
                        line.contains("quarantined")
                            && line.contains(head)
                            && line.contains(&reason)
                    })
                })
            },
            P2P_TIMEOUT,
            Duration::from_millis(200),
            "relay write was not quarantined for lack of permission",
        )
        .await;
        let unchanged = origin
            .query_with_identity(&query, &owner_key)
            .expect("origin still readable");
        assert_eq!(unchanged["User"][0]["age"], 20);
        assert_eq!(unchanged["User"][0]["name"], "Eve");
    }
}

#[tokio::test]
#[serial]
async fn owner_update_elsewhere_requires_relay_write_permission() {
    owner_edit_on_relay(false, false).await;
}

#[tokio::test]
#[serial]
async fn owner_delete_elsewhere_requires_relay_write_permission() {
    owner_edit_on_relay(true, false).await;
}

#[tokio::test]
#[serial]
async fn owner_update_elsewhere_with_granted_relay_syncs() {
    owner_edit_on_relay(false, true).await;
}

#[tokio::test]
#[serial]
async fn owner_delete_elsewhere_with_granted_relay_syncs() {
    owner_edit_on_relay(true, true).await;
}
