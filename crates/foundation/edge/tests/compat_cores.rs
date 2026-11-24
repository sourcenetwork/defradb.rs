#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
use edge::{compat, EdgeConfig, EdgeRuntime};

#[cfg(target_os = "linux")]
#[test]
fn the_compat_runtime_runs_every_thread_on_the_whole_edge_set() {
    let runtime = EdgeRuntime::start(EdgeConfig {
        thread_name: "edge-compat".to_string(),
        drain: Duration::from_secs(5),
        stop: Duration::from_secs(1),
        ..EdgeConfig::default()
    })
    .unwrap();
    let edge = runtime.edge().clone();
    let held = edge.cpus().clone();
    if held.is_empty() {
        runtime.shutdown().unwrap();
        return;
    }
    let (worker, blocking) = edge
        .block_on(edge.spawn(compat(async {
            let worker = tokio::spawn(async { workers::current_affinity() });
            let blocking = tokio::task::spawn_blocking(workers::current_affinity);
            (worker.await.unwrap(), blocking.await.unwrap())
        })))
        .unwrap()
        .unwrap();
    assert_eq!(worker.expect("the worker reads its affinity"), held);
    assert_eq!(
        blocking.expect("the blocking thread reads its affinity"),
        held
    );
    let names = std::fs::read_dir("/proc/self/task")
        .unwrap()
        .filter_map(|task| std::fs::read_to_string(task.unwrap().path().join("comm")).ok())
        .collect::<Vec<_>>();
    assert!(
        names.iter().any(|name| name.starts_with("edge-compat-tok")),
        "the compat runtime's threads carry the edge's name: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name.starts_with("async-compat")),
        "no runtime outside the edge: {names:?}"
    );
    let other = workers::EdgeCores::reserve_on(&workers::topology::Topology::detect());
    assert!(other.cpus().intersection(&held).is_empty(), "held alone");
    drop(other);
    runtime.shutdown().unwrap();
}
