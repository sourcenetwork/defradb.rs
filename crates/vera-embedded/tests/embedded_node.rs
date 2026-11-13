//! The transports against a Vera node hosted in the test process.
//!
//! The node serves no RPC socket: everything the provider reads or submits
//! goes through its in-process method table. The tests run on nuclei, which
//! is not the runtime the node runs on.

// The layout of the node's start future is deeper than the default limit.
#![recursion_limit = "256"]

use std::path::Path;
use std::sync::Arc;

use commonware_codec::Encode as _;
use futures::StreamExt as _;
use serde_json::json;
use vera::vera_rs::{
    AcpTransport, ClientError, NativeWorker, RpcTransport, TransportError, VeraRsProvider,
};
use vera::{AcpTuning, VeraProvider};
use vera_embedded::{EmbeddedError, EmbeddedTransport};
use vera_node::{EmbeddedConfig, EmbeddedNode, SoloGenesis};

const DEPLOYMENT: u64 = 9402;
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const POLICY: &str = "\
name: embedded
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

/// A single-validator node in `directory`, with its consensus listener on a
/// loopback port the system picks and no RPC listener.
async fn start(directory: &Path) -> EmbeddedNode {
    let genesis = SoloGenesis {
        chain_id: DEPLOYMENT,
        operators: serde_json::from_value(json!({ "threshold": 1, "keys": [ACCOUNT_KEY] }))
            .expect("operator policy"),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs(),
    };
    let config = EmbeddedConfig::new(directory, genesis, "127.0.0.1:0".parse().expect("address"));
    assert!(config.rpc_listen.is_none());
    EmbeddedNode::start(config).await.expect("embedded node")
}

#[nuclei::test]
async fn a_provider_creates_and_reads_back_a_policy_through_the_embedded_node() {
    let directory = tempfile::tempdir().unwrap();
    let node = start(&directory.path().join("vera")).await;
    let transport = Arc::new(EmbeddedTransport::new(node.rpc().clone()));

    let keyring = keyring::FileKeyring::open(directory.path().join("keys"), b"test-password")
        .expect("keyring");
    let worker =
        NativeWorker::open(&directory.path().join("worker"), &keyring, DEPLOYMENT).expect("worker");
    let provider = VeraRsProvider::with_transport(
        transport.clone(),
        transport,
        &hex::encode(node.consensus_key().encode()),
        &[42; 32],
        worker,
        &AcpTuning::default(),
        None,
        &edge::testing::edge(),
    )
    .await
    .expect("provider");
    assert!(
        provider
            .acp_light_client_status()
            .expect("status")
            .connected
    );

    let policy = provider.create_policy(POLICY).await.expect("policy");
    let read = provider
        .query_policy(&policy)
        .await
        .expect("policy proof")
        .expect("policy exists");
    assert_eq!(read.id, policy);
    assert_eq!(read.raw_policy.as_deref(), Some(POLICY));

    let owner = provider.self_did().expect("actor DID");
    let delegation = provider
        .create_bearer_token(&owner)
        .await
        .expect("delegation");
    provider
        .register_object(&delegation, &policy, "document", "first")
        .await
        .expect("registration");
    assert_eq!(
        provider
            .query_object_owner(&policy, "document", "first")
            .await
            .expect("owner proof"),
        (true, owner.clone())
    );
    assert!(provider
        .verify_access(&policy, "document", "first", "read", &owner)
        .await
        .expect("permission proof"));
    let stranger = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";
    assert!(!provider
        .verify_access(&policy, "document", "first", "read", stranger)
        .await
        .expect("permission proof"));

    drop(provider);
    node.stop().await.expect("stop");
}

#[nuclei::test]
async fn answers_limits_and_failures_arrive_as_they_do_over_the_network() {
    let directory = tempfile::tempdir().unwrap();
    let node = start(directory.path()).await;
    let transport = EmbeddedTransport::new(node.rpc().clone());

    let status = RpcTransport::call(&transport, "vera_nodeStatus", json!([]), 4096)
        .await
        .expect("node status");
    assert!(status.is_object());
    assert_eq!(
        AcpTransport::call(&transport, "vera_nodeStatus", json!([]), 4096)
            .await
            .expect("node status")
            .as_object()
            .map(|status| status.keys().collect::<Vec<_>>()),
        status
            .as_object()
            .map(|status| status.keys().collect::<Vec<_>>())
    );

    // The same answer under a limit it exceeds.
    assert!(matches!(
        RpcTransport::call(&transport, "vera_nodeStatus", json!([]), 8).await,
        Err(ClientError::InvalidResponse("response exceeds byte limit"))
    ));
    assert!(matches!(
        AcpTransport::call(&transport, "vera_nodeStatus", json!([]), 8).await,
        Err(TransportError::ResponseTooLarge {
            method: "vera_nodeStatus"
        })
    ));

    // The node's own refusals keep the codes a network client gets.
    assert!(matches!(
        RpcTransport::call(&transport, "vera_noSuchMethod", json!([]), 4096).await,
        Err(ClientError::Rpc { code: -32601, .. })
    ));
    assert!(matches!(
        AcpTransport::call(&transport, "vera_getLightBlock", json!(["no height"]), 4096).await,
        Err(TransportError::Rpc {
            method: "vera_getLightBlock",
            code: -32602,
            ..
        })
    ));

    let mut headers = transport.subscribe_headers().await.expect("subscription");
    let first = headers.next().await.expect("header").expect("header");
    let second = headers.next().await.expect("header").expect("header");
    assert!(first.height >= 1);
    assert!(second.height > first.height);

    node.stop().await.expect("stop");

    let stopped = RpcTransport::call(&transport, "vera_nodeStatus", json!([]), 4096)
        .await
        .expect_err("stopped node");
    assert!(matches!(
        &stopped,
        ClientError::Transport {
            retryable: false,
            ..
        }
    ));
    assert_eq!(
        stopped.to_string(),
        "transport: vera_nodeStatus: the embedded Vera node has stopped"
    );
    for stopped in [
        AcpTransport::call(&transport, "vera_nodeStatus", json!([]), 4096)
            .await
            .expect_err("stopped node"),
        transport
            .subscribe_headers()
            .await
            .err()
            .expect("stopped node"),
    ] {
        let TransportError::Other(source) = &stopped else {
            panic!("expected a channel failure, got {stopped:?}");
        };
        assert!(matches!(
            source.downcast_ref::<EmbeddedError>(),
            Some(EmbeddedError::Stopped { .. })
        ));
    }
    assert!(headers.next().await.is_none());
}
