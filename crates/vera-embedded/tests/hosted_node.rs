//! A Vera node its host starts, places and stops.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use vera::vera_rs::{NativeWorker, VeraRsProvider};
use vera::{AcpTuning, VeraProvider};
use vera_embedded::{Deployment, HostConfig, HostedNode};

const DEPLOYMENT: u64 = 9404;
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const POLICY: &str = "\
name: hosted
resources:
  - name: document
    permissions:
      - name: read
        expr: reader
    relations:
      - name: reader
        types:
          - actor
";

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

#[nuclei::test]
async fn once_the_node_stops_no_access_decision_is_given() {
    let directory = tempfile::tempdir().unwrap();
    let edge = edge::testing::edge();
    let node = HostedNode::start(config(&directory.path().join("vera")), &edge)
        .await
        .expect("hosted node");
    assert_eq!(node.chain_id(), DEPLOYMENT);

    let tuning = AcpTuning::default();
    let transport = Arc::new(node.transport(tuning.request_timeout));
    let keyring = keyring::FileKeyring::open(directory.path().join("keys"), b"test-password")
        .expect("keyring");
    let worker =
        NativeWorker::open(&directory.path().join("worker"), &keyring, DEPLOYMENT).expect("worker");
    let provider = VeraRsProvider::with_transport(
        transport.clone(),
        transport,
        node.consensus_key(),
        &[42; 32],
        worker,
        &tuning,
        None,
        &edge,
    )
    .await
    .expect("provider");
    let policy = provider.create_policy(POLICY).await.expect("policy");
    let owner = provider.self_did().expect("actor DID");
    let delegation = provider
        .create_bearer_token(&owner)
        .await
        .expect("delegation");
    provider
        .register_object(&delegation, &policy, "document", "first")
        .await
        .expect("registration");
    assert!(provider
        .verify_access(&policy, "document", "first", "read", &owner)
        .await
        .expect("permission proof"));

    node.stop().await.expect("stop");

    // The owner was admitted a moment ago. With the node gone the provider
    // has nothing to verify a decision against, and gives none.
    assert!(provider
        .verify_access(&policy, "document", "first", "read", &owner)
        .await
        .is_err());
}

#[nuclei::test]
async fn a_stopped_nodes_deployment_is_reopened_by_the_next() {
    let directory = tempfile::tempdir().unwrap();
    let edge = edge::testing::edge();
    let first = HostedNode::start(config(directory.path()), &edge)
        .await
        .expect("hosted node");
    let key = first.consensus_key().to_owned();
    // A transport that outlives the node does not keep its directory.
    let transport = first.transport(Duration::from_secs(5));
    first.stop().await.expect("stop");
    assert!(matches!(
        vera::vera_rs::RpcTransport::call(
            &transport,
            "vera_nodeStatus",
            serde_json::json!([]),
            4096
        )
        .await,
        Err(vera::vera_rs::ClientError::Transport {
            retryable: false,
            ..
        })
    ));

    let second = HostedNode::start(config(directory.path()), &edge)
        .await
        .expect("reopened node");
    assert_eq!(second.consensus_key(), key);
    second.stop().await.expect("stop");
}

#[nuclei::test]
async fn a_start_over_a_held_directory_fails_at_its_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let edge = edge::testing::edge();
    let first = HostedNode::start(config(directory.path()), &edge)
        .await
        .expect("hosted node");

    let mut held = config(directory.path());
    held.start_deadline = Duration::from_secs(2);
    let refused = HostedNode::start(held, &edge).await;
    assert!(
        matches!(
            &refused,
            Err(vera_embedded::HostError::StartElapsed { directory: named, .. })
                if named == directory.path()
        ),
        "a start over a held directory fails at its deadline, got {refused:?}"
    );

    // The start that gave up left nothing behind: once the first node lets
    // go, the directory opens again.
    first.stop().await.expect("stop");
    let third = HostedNode::start(config(directory.path()), &edge)
        .await
        .expect("the directory is free again");
    third.stop().await.expect("stop");
}

#[test]
fn an_operator_policy_the_node_would_refuse_fails_the_start() {
    let directory = tempfile::tempdir().unwrap();
    let edge = edge::testing::edge();
    let mut config = config(directory.path());
    config.deployment.threshold = 2;
    let refused = edge
        .block_on(HostedNode::start(config, &edge))
        .expect("the network runtime runs the start");
    assert!(matches!(refused, Err(vera_embedded::HostError::Start(_))));
}

/// Every thread the node starts runs on the network runtime's CPUs: none
/// shares a core with a database worker.
///
/// The threads are told apart by when they appear, so the test needs a
/// process of its own, as nextest gives it: under `cargo test` another
/// test's threads start in the same window.
#[cfg(target_os = "linux")]
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
