#[path = "../benches/write_baseline/workload.rs"]
mod workload;

#[nuclei::test(threads = 4)]
async fn concurrent_workloads_account_for_every_request_and_persist_their_writes() {
    for scenario in workload::Scenario::ALL {
        let result = workload::run(&edge::testing::edge(), scenario, 4, 4).await;
        assert_eq!(result.latencies.len(), 16);
        assert_eq!(
            result.commits + result.client_conflicts + result.engine_exhaustions,
            16
        );
        assert!(result.commits > 0);
        assert!(!result.elapsed.is_zero());
        assert_eq!(
            result.engine_retries,
            result.retries_after.engine.attempts - result.retries_before.engine.attempts,
            "benchmark engine retry metric counts replay executions"
        );
        assert_eq!(result.retries_after.http_auto_commit, Default::default());
        assert_eq!(result.retries_after.embedded_execute, Default::default());
        assert!(result.latencies.iter().all(|latency| !latency.is_zero()));
    }
}

#[test]
fn scenario_filter_rejects_an_unknown_name() {
    let result = std::panic::catch_unwind(|| workload::selected(Some("create")));
    assert!(result.is_err(), "a typo must not select no workload");
}

#[test]
fn scenario_filter_keeps_only_named_workloads() {
    let selected = workload::selected(Some("creates,counters"));
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0].name(), "creates");
    assert_eq!(selected[1].name(), "counters");
}
