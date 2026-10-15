#[path = "../benches/write_baseline/workload.rs"]
mod workload;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_workloads_account_for_every_request_and_persist_their_writes() {
    for scenario in workload::Scenario::ALL {
        let result = workload::run(scenario, 4, 4).await;
        assert_eq!(result.latencies.len(), 16);
        assert_eq!(result.commits + result.exhaustions, 16);
        assert!(result.commits > 0);
        assert!(!result.elapsed.is_zero());
        assert!(
            result.storage_conflicts
                <= result.retries_after.http_auto_commit.attempts
                    - result.retries_before.http_auto_commit.attempts
                    + result.exhaustions
        );
        assert_eq!(
            result.retries_before.embedded_execute,
            result.retries_after.embedded_execute
        );
        assert!(result.latencies.iter().all(|latency| !latency.is_zero()));
    }
}
