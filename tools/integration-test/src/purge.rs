use std::time::Duration;

use defra_harness::cluster::health::health_check;
use defra_harness::TestCluster;

const RESTART_TIMEOUT: Duration = Duration::from_secs(30);

/// Purge the node at `index` and wait until it serves requests again.
///
/// Go answers the purge request and then restarts its HTTP server, so a
/// request sent straight after the purge can find nothing listening.
pub async fn purge_node(cluster: &TestCluster, index: usize) {
    cluster
        .client(index)
        .purge()
        .unwrap_or_else(|error| panic!("purge node {index}: {error}"));
    health_check(
        &reqwest::Client::new(),
        cluster.api_url(index),
        RESTART_TIMEOUT,
    )
    .await
    .unwrap_or_else(|error| panic!("node {index} did not come back after purge: {error}"));
}
