//! What the transport costs a provider call, apart from the node's pacing.
//!
//! Three setups run the same calls: a Vera daemon over HTTP, a node in this
//! process over HTTP, and that kind of node through its in-process method
//! table. The second and third share a node configuration, so their
//! difference is the socket; the first differs from the second by the
//! daemon's block pacing. Run it with
//! `cargo test -p integration-test --test verars transport_latency -- --ignored --nocapture`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use commonware_codec::Encode as _;
use integration_test::USER_ACP_POLICY;
use vera::{AcpTuning, VeraProvider, VeraRsProvider};

use super::embedded::{self, Embedded};
use super::helpers;

const WRITES: usize = 20;
const READS: usize = 100;

/// The median and the 95th percentile of `samples`, in milliseconds.
fn summary(mut samples: Vec<Duration>) -> serde_json::Value {
    samples.sort();
    let at = |quantile: f64| {
        let index = ((samples.len() - 1) as f64 * quantile).round() as usize;
        (samples[index].as_secs_f64() * 1_000.0 * 100.0).round() / 100.0
    };
    serde_json::json!({ "p50_ms": at(0.5), "p95_ms": at(0.95), "samples": samples.len() })
}

/// One policy, then `WRITES` registrations and `READS` proven access checks.
async fn measure(setup: &str, provider: Arc<VeraRsProvider>) -> serde_json::Value {
    let owner = provider.self_did().expect("actor DID");
    let policy = provider
        .create_policy(USER_ACP_POLICY)
        .await
        .expect("policy");
    let token = provider
        .create_bearer_token(&owner)
        .await
        .expect("delegation");
    let mut writes = Vec::with_capacity(WRITES);
    for index in 0..WRITES {
        let started = Instant::now();
        provider
            .register_object(&token, &policy, "users", &format!("document-{index}"))
            .await
            .expect("registration");
        writes.push(started.elapsed());
    }
    let mut reads = Vec::with_capacity(READS);
    for index in 0..READS {
        let object = format!("document-{}", index % WRITES);
        let started = Instant::now();
        assert!(provider
            .verify_access(&policy, "users", &object, "owner", &owner)
            .await
            .expect("permission proof"));
        reads.push(started.elapsed());
    }
    serde_json::json!({
        "setup": setup,
        "certified_write": summary(writes),
        "verified_read": summary(reads),
    })
}

/// The setups, in the order a round measures them.
const SETUPS: [&str; 3] = [
    "daemon over HTTP",
    "embedded over HTTP",
    "embedded in process",
];

/// Starts the node of `setup`, measures it and stops the node.
async fn measured(setup: &'static str) -> serde_json::Value {
    let root = tempfile::tempdir().unwrap();
    if setup == SETUPS[0] {
        let hub = helpers::start_hub_cluster().await;
        let keys = vera_harness::cluster::KeySet::builder()
            .nodes(1)
            .seed(0)
            .build()
            .expect("bootstrap keys");
        let consensus_key = hex::encode(keys.epoch_info().output.public().public().encode());
        let owner = helpers::funded_identity();
        let daemon = VeraRsProvider::new(
            hub.node(0).rpc_url(),
            &consensus_key,
            &hex::decode(&owner.private_key_hex).expect("owner key"),
            embedded::worker(root.path()),
            &AcpTuning::default(),
            None,
            &edge::testing::edge(),
        )
        .await
        .expect("provider");
        return measure(setup, Arc::new(daemon)).await;
    }
    let node = Embedded::start().await;
    let worker = embedded::worker(root.path());
    let provider = if setup == SETUPS[1] {
        node.over_http(worker).await
    } else {
        node.in_process(worker).await
    };
    let row = measure(setup, Arc::new(provider.expect("provider"))).await;
    node.stop().await;
    row
}

#[nuclei::test]
#[serial_test::serial]
#[ignore = "a measurement, not a check: run it by name"]
async fn transport_latency() {
    edge::compat(async {
        // Two rounds, the second in the opposite order, so a setup is not
        // always the one measured on a cold or a warm machine.
        let forward = SETUPS.to_vec();
        let backward: Vec<_> = SETUPS.iter().rev().copied().collect();
        for (round, order) in [forward, backward].into_iter().enumerate() {
            for (position, setup) in order.into_iter().enumerate() {
                let mut row = measured(setup).await;
                row["round"] = (round + 1).into();
                row["position"] = (position + 1).into();
                println!("{row}");
            }
        }
    })
    .await;
}
