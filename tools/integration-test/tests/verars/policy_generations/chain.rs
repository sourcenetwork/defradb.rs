//! Waits on the chain: a node's verified ACP height reaching what a write
//! or a policy edit committed.

use std::time::Duration;

use integration_test::TestCluster;

async fn verified_height(http: &net::http::Client, url: &str) -> u64 {
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

fn http_client() -> net::http::Client {
    net::http::Client::builder(edge::testing::edge())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

pub(super) async fn wait_for_height(cluster: &TestCluster, minimum: u64) {
    let http = http_client();
    for index in 0..cluster.len() {
        nuclei::time::timeout(Duration::from_secs(30), async {
            loop {
                let height = verified_height(&http, cluster.api_url(index)).await;
                if height >= minimum {
                    eprintln!("replica {index}: verified ACP height {height} >= {minimum}");
                    break;
                }
                nuclei::time::sleep(Duration::from_millis(50)).await;
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
