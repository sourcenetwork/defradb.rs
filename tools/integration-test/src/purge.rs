use std::time::Duration;

use defra_harness::cluster::health::health_check;
use defra_harness::{NodeKind, TestCluster};

const RESTART_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_GRACE: Duration = Duration::from_millis(500);
const SHUTDOWN_POLL: Duration = Duration::from_millis(5);

/// Purge the node at `index` and wait until it serves requests again.
///
/// Go answers the purge request and then restarts its HTTP server, so a
/// request sent straight after the purge can find nothing listening.
pub async fn purge_node(cluster: &TestCluster, index: usize) {
    let client = cluster.client(index);
    client
        .purge()
        .unwrap_or_else(|error| panic!("purge node {index}: {error}"));

    let http = reqwest::Client::new();
    let url = cluster.api_url(index);
    if client.kind() == NodeKind::Go {
        wait_until_down(&http, url).await;
    }
    health_check(&http, url, RESTART_TIMEOUT)
        .await
        .unwrap_or_else(|error| panic!("node {index} did not come back after purge: {error}"));
}

/// Wait for the pre-purge server to stop answering, so the health check that
/// follows cannot be answered by it.
///
/// A restart that completes between two polls is never seen as down; the
/// grace period then runs out and the health check finds the new server.
async fn wait_until_down(http: &reqwest::Client, url: &str) {
    let health_url = format!("{url}/health-check");
    let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
    while tokio::time::Instant::now() < deadline {
        match http.get(&health_url).send().await {
            Ok(response) if response.status().is_success() => {}
            _ => return,
        }
        tokio::time::sleep(SHUTDOWN_POLL).await;
    }
}
