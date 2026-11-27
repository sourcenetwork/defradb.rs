//! A read while the embedded node's newest verified revision is older than
//! the light client's freshness limit, as on a starved host.
//!
//! The headers the node finalizes are held back from the provider until its
//! newest revision has aged past the limit; a read then waits for the next
//! revision instead of failing, and is answered once headers flow again.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use defra_core::thread_bounds::MaybeBoxFuture;
use futures::future::BoxFuture;
use futures::StreamExt as _;
use serde_json::Value;
use vera::vera_rs::{
    AcpTransport, ClientError, GenesisSource, HeaderStream, NativeWorker, RpcTransport,
    TransportError, VeraRsProvider,
};
use vera::{AcpTuning, VeraProvider};
use vera_embedded::{Deployment, EmbeddedTransport, HostConfig, HostedNode};

const DEPLOYMENT: u64 = 9405;
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const POLICY: &str = "\
name: stale
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
/// Past the light client's freshness limit of 30 seconds.
const STARVED: Duration = Duration::from_secs(32);

/// The node's transport, whose finalized headers can be held back.
struct Starving {
    inner: EmbeddedTransport,
    held: Arc<AtomicBool>,
}

impl RpcTransport for Starving {
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: Value,
        max_bytes: usize,
    ) -> MaybeBoxFuture<'a, Result<Value, ClientError>> {
        RpcTransport::call(&self.inner, method, params, max_bytes)
    }
}

impl AcpTransport for Starving {
    fn call(
        &self,
        method: &'static str,
        params: Value,
        max_bytes: usize,
    ) -> BoxFuture<'_, Result<Value, TransportError>> {
        AcpTransport::call(&self.inner, method, params, max_bytes)
    }

    fn subscribe_headers(&self) -> BoxFuture<'_, Result<HeaderStream, TransportError>> {
        let held = Arc::clone(&self.held);
        Box::pin(async move {
            let headers = self.inner.subscribe_headers().await?;
            let passed = headers.filter(move |_| {
                let pass = !held.load(Ordering::Relaxed);
                async move { pass }
            });
            Ok(Box::pin(passed) as HeaderStream)
        })
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[nuclei::test]
async fn a_read_at_a_starved_head_waits_for_the_next_revision() {
    let directory = tempfile::tempdir().unwrap();
    let edge = edge::testing::edge();
    let node = HostedNode::start(
        HostConfig {
            data_dir: directory.path().join("vera"),
            deployment: Deployment {
                chain_id: DEPLOYMENT,
                threshold: 1,
                operator_keys: vec![ACCOUNT_KEY.to_string()],
                created: now(),
                policies: Vec::new(),
            },
            device: vera_embedded::HostDevice::Alone,
            listen: "127.0.0.1:0".parse().unwrap(),
            start_deadline: Duration::from_secs(60),
            stop_deadline: Duration::from_secs(30),
        },
        &edge,
    )
    .await
    .expect("hosted node");
    let tuning = AcpTuning::default();
    let held = Arc::new(AtomicBool::new(false));
    let transport = Arc::new(Starving {
        inner: node.transport(tuning.request_timeout),
        held: Arc::clone(&held),
    });
    let keyring =
        keyring::FileKeyring::open(directory.path().join("keys"), b"test-password").unwrap();
    let worker =
        NativeWorker::open(&directory.path().join("worker"), &keyring, DEPLOYMENT).unwrap();
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
    let policy = provider.create_policy(POLICY, None).await.expect("policy");
    let owner = provider.self_did().unwrap();
    let delegation = provider.create_bearer_token(&owner).await.unwrap();
    provider
        .register_object(&delegation, &policy, "document", "first", &[])
        .await
        .expect("registration");

    // The host is starved: no header reaches the light client for longer
    // than its freshness limit.
    held.store(true, Ordering::Relaxed);
    edge.sleep(STARVED).await;
    assert!(provider.decision_state().is_none(), "the head is stale");

    let release = {
        let held = Arc::clone(&held);
        let edge = edge.clone();
        async move {
            edge.sleep(Duration::from_secs(2)).await;
            held.store(false, Ordering::Relaxed);
        }
    };
    let started = Instant::now();
    let (allowed, ()) = futures::join!(
        provider.verify_access(&policy, "document", "first", "read", &owner),
        release
    );
    assert!(
        allowed.expect("answered once a fresh revision arrived"),
        "the owner reads"
    );
    assert!(started.elapsed() >= Duration::from_secs(2), "it waited");
    node.stop().await.expect("stop");
}
