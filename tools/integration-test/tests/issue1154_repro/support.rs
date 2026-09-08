use integration_test::TestCluster;

use super::PUSHERS;

pub(super) async fn pending_dags(hub_api: &str) -> u64 {
    let Ok(response) = reqwest::get(format!("{hub_api}/api/v0/p2p/sync/status")).await else {
        return 0;
    };
    response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|status| status["pending_dags"].as_u64())
        .unwrap_or(0)
}

pub(super) async fn sync_status(cluster: &TestCluster, node: usize) -> serde_json::Value {
    reqwest::get(format!("{}/api/v0/p2p/sync/status", cluster.api_url(node)))
        .await
        .expect("sync status request")
        .json()
        .await
        .expect("sync status json")
}

pub(super) async fn sender_retry_snapshot(cluster: &TestCluster) -> (usize, u64) {
    let mut markers = 0usize;
    let mut active_jobs = 0u64;
    for pusher in 1..=PUSHERS {
        let status = sync_status(cluster, pusher).await;
        markers += status["push_retry_markers"]["document_markers"]
            .as_u64()
            .expect("document marker count") as usize;
        active_jobs += status["push_backlog"]["active_jobs"]
            .as_u64()
            .expect("active sender jobs");
    }
    (markers, active_jobs)
}

pub(super) fn log_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|value| value.strip_prefix(field))
}

/// Documents the hub durably registered before acknowledging their push.
///
/// This log proves persistence, not delivery of the acknowledgment. The test
/// separately correlates the pending root with a sender-side success reply.
/// An at-capacity push returns before this log. Each document is created once
/// and never updated, so one document maps to at most one live root.
pub(super) fn registered_doc_ids(hub_log: &std::path::Path) -> std::collections::HashSet<String> {
    std::fs::read_to_string(hub_log)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("Persisted pending DAG registration"))
        .filter_map(|line| log_field(line, "doc_id=").map(str::to_string))
        .collect()
}
