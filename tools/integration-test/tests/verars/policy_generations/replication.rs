use std::time::Duration;

use integration_test::{DefraClient, TestCluster};

use super::commit_ids;

pub(super) fn connect(nodes: &[DefraClient], owner: &str) {
    let addresses = nodes[1].p2p_info().expect("receiver P2P addresses");
    let address = addresses
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.as_str())
        .expect("receiver P2P address");
    nodes[0].p2p_connect(&[address]).expect("connect replicas");
    for node in nodes {
        node.p2p_collection_add(&["User"])
            .expect("subscribe to protected collection");
    }
    nodes[0]
        .p2p_replicator_set_with_identity(&["User"], address, owner)
        .expect("authorize protected collection replication");
}

async fn verified_height(http: &reqwest::Client, url: &str) -> u64 {
    let status: serde_json::Value = http
        .get(format!("{url}/api/v0/acp/status"))
        .send()
        .await
        .expect("ACP status transport")
        .error_for_status()
        .expect("ACP status response")
        .json()
        .await
        .expect("ACP status JSON");
    status["height"].as_u64().expect("verified ACP height")
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

pub(super) async fn wait_for_height(cluster: &TestCluster, minimum: u64) {
    let http = http_client();
    for index in 0..cluster.len() {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let height = verified_height(&http, cluster.api_url(index)).await;
                if height >= minimum {
                    eprintln!("replica {index}: verified ACP height {height} >= {minimum}");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("replica {index} did not verify ACP height {minimum}"));
    }
}

pub(super) async fn wait_for_submission(cluster: &TestCluster, submitting_node: usize) {
    // Successful ACP writes wait for a certified receipt AND the submitting provider's
    // verified chain to reach it. Its subsequent status therefore supplies a lower bound
    // covering that mutation; neither replica may use older evidence in the assertions.
    let minimum = verified_height(&http_client(), cluster.api_url(submitting_node)).await;
    assert!(minimum > 0, "submission must have a verified revision");
    wait_for_height(cluster, minimum).await;
}

pub(super) async fn wait_for_document(
    receiver: &DefraClient,
    document: &str,
    owner: &str,
    expected_commits: &[String],
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let result = receiver
            .query_with_identity("query { User { _docID name } }", owner)
            .expect("owner reads replicated document");
        let users = result["User"].as_array().expect("replicated User array");
        let actual_commits = if users.len() == 1 && users[0]["_docID"] == document {
            commit_ids(receiver, document, Some(owner))
        } else {
            Vec::new()
        };
        if users.len() == 1
            && users[0]["_docID"] == document
            && users[0]["name"] == "Alice"
            && actual_commits == expected_commits
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "receiver did not converge to document {document} and exact CIDs: expected {expected_commits:?}, got {actual_commits:?}; document result {result}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
