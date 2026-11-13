//! Where a hosted Vera node's threads run.
//!
//! The node's threads are told apart by when they appear, so this is the
//! only test of its binary: no other test starts threads beside it, under
//! `cargo test` or nextest.

#![cfg(target_os = "linux")]

use std::path::Path;
use std::time::Duration;

use vera_embedded::{Deployment, HostConfig, HostedNode};

const DEPLOYMENT: u64 = 9406;
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

fn config(directory: &Path) -> HostConfig {
    HostConfig {
        data_dir: directory.to_path_buf(),
        deployment: Deployment {
            chain_id: DEPLOYMENT,
            threshold: 1,
            operator_keys: vec![ACCOUNT_KEY.to_string()],
            created: now(),
        },
        listen: "127.0.0.1:0".parse().expect("address"),
        start_deadline: Duration::from_secs(60),
        stop_deadline: Duration::from_secs(30),
    }
}
/// Every thread the node starts runs on the network runtime's CPUs: none
/// shares a core with a database worker.
#[test]
fn every_thread_of_the_node_runs_on_the_network_runtimes_cpus() {
    use std::collections::BTreeSet;

    fn threads() -> BTreeSet<u32> {
        std::fs::read_dir("/proc/self/task")
            .expect("thread list")
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
            .collect()
    }

    let runtime = edge::EdgeRuntime::start(edge::EdgeConfig {
        thread_name: "vera-host-cores".to_string(),
        ..edge::EdgeConfig::default()
    })
    .expect("network runtime");
    let edge = runtime.edge().clone();
    let held = edge.cpus().clone();
    if held.is_empty() {
        // The machine has no cores to hold apart; nothing is placed.
        runtime.shutdown().expect("shutdown");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let before = threads();
    let node = edge
        .block_on(HostedNode::start(config(directory.path()), &edge))
        .expect("the network runtime runs the start")
        .expect("hosted node");

    let mut placed = 0;
    let mut node_thread = false;
    for thread in threads().difference(&before) {
        let task = format!("/proc/self/task/{thread}");
        // A thread that exited since it was listed has no status to read.
        let Ok(status) = std::fs::read_to_string(format!("{task}/status")) else {
            continue;
        };
        let name = std::fs::read_to_string(format!("{task}/comm")).unwrap_or_default();
        let list = status
            .lines()
            .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
            .expect("affinity")
            .trim();
        let cpus = workers::topology::CpuSet::parse(list).expect("CPU list");
        assert!(
            cpus.iter().all(|cpu| held.contains(cpu)),
            "thread {} runs on {cpus}, outside {held}",
            name.trim()
        );
        placed += 1;
        node_thread |= name.trim() == "vera-node";
    }
    assert!(node_thread, "the node thread is among those placed");
    assert!(
        placed >= node.threads().persistent(),
        "{placed} threads placed, {} expected",
        node.threads().persistent()
    );

    edge.block_on(node.stop())
        .expect("the network runtime runs the stop")
        .expect("stop");
    runtime.shutdown().expect("shutdown");
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}
