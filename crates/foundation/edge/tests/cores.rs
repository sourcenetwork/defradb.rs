use std::num::NonZeroUsize;
use std::time::Duration;

use edge::{EdgeConfig, EdgeRuntime};
use workers::Placement;

fn start(threads: Option<NonZeroUsize>, placement: Placement) -> EdgeRuntime {
    EdgeRuntime::start(EdgeConfig {
        threads,
        placement,
        thread_name: "edge-cores".to_string(),
        drain: Duration::from_secs(5),
        stop: Duration::from_secs(1),
        ..EdgeConfig::default()
    })
    .unwrap()
}

/// Every shard of `runtime` runs wherever the calling thread may.
#[cfg(target_os = "linux")]
fn every_shard_runs_wherever_the_process_may(runtime: &EdgeRuntime) {
    let edge = runtime.edge().clone();
    let mask = edge
        .block_on(edge.spawn(async { workers::current_affinity() }))
        .unwrap()
        .unwrap()
        .expect("the shard reads its affinity");
    assert_eq!(
        Some(mask),
        workers::current_affinity(),
        "a shard runs wherever the process may"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_runtime_sized_from_the_machine_runs_every_thread_on_cores_of_its_own() {
    let runtime = start(None, Placement::default());
    let edge = runtime.edge().clone();
    let held = edge.cpus().clone();
    if held.is_empty() {
        runtime.shutdown().unwrap();
        return;
    }
    let spawns = held.len() * 4;
    let shards = edge
        .block_on({
            let edge = edge.clone();
            async move {
                let tasks: Vec<_> = (0..spawns)
                    .map(|_| edge.spawn(async { workers::current_affinity() }))
                    .collect();
                let mut seen = Vec::new();
                for task in tasks {
                    seen.push(task.await.unwrap());
                }
                seen
            }
        })
        .unwrap();
    for cpus in shards {
        let cpus = cpus.expect("the shard reads its affinity");
        assert_eq!(cpus.len(), 1, "one shard per CPU: {cpus}");
        assert!(
            held.contains(cpus.first().unwrap()),
            "{cpus} outside {held}"
        );
    }
    let blocking = edge
        .block_on(edge.spawn_blocking(workers::current_affinity))
        .unwrap()
        .unwrap()
        .expect("the blocking thread reads its affinity");
    assert!(
        blocking.iter().all(|cpu| held.contains(cpu)),
        "{blocking} outside {held}"
    );
    let other = workers::EdgeCores::reserve_on(
        &workers::topology::Topology::detect(),
        &Placement::default(),
    );
    assert!(other.cpus().intersection(&held).is_empty(), "held alone");
    drop(other);
    runtime.shutdown().unwrap();
}

#[test]
fn a_runtime_placed_unpinned_runs_every_thread_unpinned_holding_nothing() {
    let runtime = start(None, Placement::Unpinned);
    assert!(
        runtime.edge().cpus().is_empty(),
        "pinned to {}",
        runtime.edge().cpus()
    );
    #[cfg(target_os = "linux")]
    every_shard_runs_wherever_the_process_may(&runtime);
    runtime.shutdown().unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_runtime_placed_auto_on_every_online_cpu_pins_nothing() {
    if workers::topology::Topology::detect().narrowed {
        return;
    }
    let runtime = start(None, Placement::Auto);
    assert!(
        runtime.edge().cpus().is_empty(),
        "pinned to {}",
        runtime.edge().cpus()
    );
    every_shard_runs_wherever_the_process_may(&runtime);
    runtime.shutdown().unwrap();
}

#[test]
fn a_runtime_given_a_count_runs_that_many_unpinned_threads() {
    let runtime = start(NonZeroUsize::new(2), Placement::default());
    assert!(runtime.edge().cpus().is_empty());
    runtime.shutdown().unwrap();
}
