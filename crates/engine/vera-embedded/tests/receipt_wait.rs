//! A write through the provider is confirmed when the node publishes the
//! header of the block that holds it, not when the wait's fallback runs out.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vera::vera_rs::{GenesisSource, NativeWorker, VeraRsProvider};
use vera::{AcpTuning, VeraProvider};
use vera_embedded::{Deployment, HostConfig, HostedNode};

const DEPLOYMENT: u64 = 9406;
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const POLICY: &str = "\
name: receipts
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

/// Far longer than a write takes: writes that end before it never waited
/// for it.
const FALLBACK: Duration = Duration::from_secs(30);

fn config(directory: &Path) -> HostConfig {
    HostConfig {
        data_dir: directory.to_path_buf(),
        deployment: Deployment {
            chain_id: DEPLOYMENT,
            threshold: 1,
            operator_keys: vec![ACCOUNT_KEY.to_string()],
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_secs(),
            policies: Vec::new(),
        },
        listen: "127.0.0.1:0".parse().expect("address"),
        start_deadline: Duration::from_secs(60),
        stop_deadline: Duration::from_secs(30),
    }
}

#[nuclei::test]
async fn writes_are_confirmed_at_their_blocks_header_and_not_at_the_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let edge = edge::testing::edge();
    let node = HostedNode::start(config(&directory.path().join("vera")), &edge)
        .await
        .expect("hosted node");
    let tuning = AcpTuning {
        receipt_fallback: FALLBACK,
        receipt_timeout: 4 * FALLBACK,
        ..AcpTuning::default()
    };
    let transport = Arc::new(node.transport(tuning.request_timeout));
    let keyring = keyring::FileKeyring::open(directory.path().join("keys"), b"test-password")
        .expect("keyring");
    let worker =
        NativeWorker::open(&directory.path().join("worker"), &keyring, DEPLOYMENT).expect("worker");
    let provider = VeraRsProvider::with_transport(
        transport.clone(),
        transport,
        node.consensus_key(),
        GenesisSource::Hosted,
        &[42; 32],
        worker,
        &tuning,
        None,
        &edge,
    )
    .await
    .expect("provider");

    let started = Instant::now();
    let policy = provider.create_policy(POLICY, None).await.expect("policy");
    let owner = provider.self_did().expect("actor DID");
    let delegation = provider
        .create_bearer_token(&owner)
        .await
        .expect("delegation");
    for document in ["first", "second", "third"] {
        provider
            .register_object(&delegation, &policy, "document", document)
            .await
            .expect("registration");
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < FALLBACK,
        "four writes took {elapsed:?}: one waited for the fallback, not its header"
    );
    assert!(provider
        .verify_access(&policy, "document", "third", "read", &owner)
        .await
        .expect("permission proof"));

    node.stop().await.expect("stop");
}
