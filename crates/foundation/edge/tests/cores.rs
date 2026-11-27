use std::num::NonZeroUsize;
use std::time::Duration;

use edge::{EdgeConfig, EdgeRuntime};

fn start(threads: Option<NonZeroUsize>) -> EdgeRuntime {
    EdgeRuntime::start(EdgeConfig {
        threads,
        thread_name: "edge-cores".to_string(),
        drain: Duration::from_secs(5),
        stop: Duration::from_secs(1),
        ..EdgeConfig::default()
    })
    .unwrap()
}

#[cfg(target_os = "linux")]
#[test]
fn a_runtime_sized_from_the_machine_runs_every_thread_on_cores_of_its_own() {
    let runtime = start(None);
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
    let other = workers::EdgeCores::reserve_on(&workers::topology::Topology::detect());
    assert!(other.cpus().intersection(&held).is_empty(), "held alone");
    drop(other);
    runtime.shutdown().unwrap();
}

#[test]
fn a_runtime_given_a_count_runs_that_many_unpinned_threads() {
    let runtime = start(NonZeroUsize::new(2));
    assert!(runtime.edge().cpus().is_empty());
    runtime.shutdown().unwrap();
}
