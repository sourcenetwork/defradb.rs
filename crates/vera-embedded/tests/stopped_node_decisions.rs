//! What document ACP answers once the Vera node behind it is gone.

// The layout of the node's start future is deeper than the default limit.
#![recursion_limit = "256"]

use std::sync::Arc;
use std::time::Duration;

use acp::{DocumentACP as _, DocumentPermission, Identity};
use commonware_codec::Encode as _;
use serde_json::json;
use vera::vera_rs::{NativeWorker, VeraRsProvider};
use vera::{AcpTuning, VeraDocumentACP, VeraProvider};
use vera_embedded::EmbeddedTransport;
use vera_node::{EmbeddedConfig, EmbeddedNode, SoloGenesis};

const DEPLOYMENT: u64 = 9408;
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const POLICY: &str = "\
name: stopped
resources:
  - name: users
    permissions:
      - name: read
        expr: writer + reader
      - name: update
        expr: writer
      - name: delete
        expr: writer
    relations:
      - name: writer
        types:
          - actor
      - name: reader
        types:
          - actor
";

#[nuclei::test]
async fn a_remembered_decision_is_not_given_past_the_freshness_limit() {
    let directory = tempfile::tempdir().unwrap();
    let genesis = SoloGenesis {
        policies: Vec::new(),
        chain_id: DEPLOYMENT,
        operators: serde_json::from_value(json!({ "threshold": 1, "keys": [ACCOUNT_KEY] }))
            .expect("operator policy"),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs(),
    };
    let node = EmbeddedNode::start(EmbeddedConfig::new(
        directory.path().join("vera"),
        genesis,
        "127.0.0.1:0".parse().expect("address"),
    ))
    .await
    .expect("embedded node");
    let tuning = AcpTuning::default();
    let edge = edge::testing::edge();
    let transport = Arc::new(EmbeddedTransport::new(
        node.rpc().clone(),
        tuning.request_timeout,
        &edge,
    ));
    let keyring = keyring::FileKeyring::open(directory.path().join("keys"), b"test-password")
        .expect("keyring");
    let worker =
        NativeWorker::open(&directory.path().join("worker"), &keyring, DEPLOYMENT).expect("worker");
    let provider = Arc::new(
        VeraRsProvider::with_transport(
            transport.clone(),
            transport,
            &hex::encode(node.consensus_key().encode()),
            &[42; 32],
            worker,
            &tuning,
            None,
            &edge,
        )
        .await
        .expect("provider"),
    );
    let owner = provider.self_did().expect("actor DID");
    let policy = provider.create_policy(POLICY).await.expect("policy");
    let delegation = provider
        .create_bearer_token(&owner)
        .await
        .expect("delegation");
    for document in ["asked", "never-asked"] {
        provider
            .register_object(&delegation, &policy, "users", document)
            .await
            .expect("registration");
    }

    let document_acp = VeraDocumentACP::without_access_cache(provider);
    let identity = Identity::authenticated(identity::Did::new(&owner).unwrap());
    let reads = |document: &'static str| {
        document_acp.check_doc_access(
            &identity,
            DocumentPermission::Read,
            &policy,
            "users",
            document,
        )
    };
    assert!(reads("asked").await.expect("decision while the node runs"));

    let stopped = std::time::Instant::now();
    node.stop().await.expect("stop");

    // A decision that was not asked before has nothing to be read from.
    assert!(reads("never-asked").await.is_err());

    // The decision made at the last verified state is exact for that state,
    // and no later than the freshness limit that state is no longer one to
    // answer from: the owner admitted a moment ago is not admitted again.
    // The limit is the light client's own default, which the provider uses.
    let freshness = acp_light_client::FreshnessPolicy::default().max_age;
    nuclei::time::sleep((freshness + Duration::from_secs(2)).saturating_sub(stopped.elapsed()))
        .await;
    let late = reads("asked").await;
    assert!(
        matches!(&late, Err(error) if error.to_string().contains("stale")),
        "a decision repeated past the freshness limit: {late:?}"
    );
}
