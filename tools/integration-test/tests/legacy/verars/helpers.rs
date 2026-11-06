use std::time::Duration;

use commonware_codec::Encode as _;

use identity::{Identity, IdentityKeyType, RawIdentity};
use integration_test::{BinarySource, TestCluster, TestClusterBuilder, TestIdentity};

pub use integration_test::vera_cli_binary as defra_binary;

const FUNDED_PRIVATE_KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

pub fn funded_identity() -> TestIdentity {
    let private_key_hex = FUNDED_PRIVATE_KEY.to_string();
    let key_bytes = hex::decode(&private_key_hex).expect("funded key hex");
    let identity = RawIdentity::from_identity_key_type(IdentityKeyType::Secp256k1, &key_bytes)
        .expect("funded identity");

    TestIdentity {
        private_key_hex,
        did: identity.did().expect("funded identity did").to_string(),
        public_key_hex: Some(hex::encode(identity.public_key_bytes())),
        key_type: Some("secp256k1".to_string()),
    }
}

/// Start a native-only pipelined Vera validator and wait for readiness.
pub async fn start_hub_cluster() -> vera_harness::cluster::TestCluster {
    let cluster = vera_harness::cluster::TestCluster::builder()
        .nodes(1)
        .seed(0)
        .genesis(vera_harness::cluster::GenesisBuilder::devnet().simplex(Default::default()))
        .build()
        .await
        .expect("start vera.rs cluster");
    cluster
        .wait_ready(Duration::from_secs(30))
        .await
        .expect("vera.rs cluster ready");
    cluster
}

/// Configure document ACP through arguments retained by the harness on restart.
pub fn vera_rs_builder(
    hub_rpc_url: &str,
    identity: &str,
    n_nodes: usize,
    p2p: bool,
) -> TestClusterBuilder {
    let keys = vera_harness::cluster::KeySet::builder()
        .nodes(1)
        .seed(0)
        .build()
        .expect("bootstrap keys");
    let trusted_key = hex::encode(keys.epoch_info().output.public().public().encode());
    let mut builder = TestCluster::builder()
        .rust_nodes(n_nodes)
        .with_keyring()
        .with_rust_binary(BinarySource::Path(defra_binary()))
        .with_extra_rust_args([
            "--vera-rs-address".to_owned(),
            hub_rpc_url.to_owned(),
            "--vera-consensus-key".to_owned(),
            trusted_key,
            "--vera-deployment-id".to_owned(),
            "9001".to_owned(),
            "--document-acp-type".to_owned(),
            "verars".to_owned(),
        ]);
    for index in 0..n_nodes {
        builder = builder.with_node_identity(index, identity.to_string());
    }
    if p2p {
        builder = builder.with_p2p();
    }
    builder
}

/// Build a DefraDB test cluster configured to use vera.rs for document ACP.
pub async fn build_defra_with_vera_rs(
    hub_rpc_url: &str,
    identity: &str,
    n_nodes: usize,
    p2p: bool,
) -> TestCluster {
    vera_rs_builder(hub_rpc_url, identity, n_nodes, p2p)
        .build()
        .await
        .expect("build defra cluster")
}

pub async fn policy_exists(hub_rpc_url: &str, policy_id: &str) -> bool {
    let keys = vera_harness::cluster::KeySet::builder()
        .nodes(1)
        .seed(0)
        .build()
        .expect("bootstrap keys");
    let response = vera_client::VeraClient::new(hub_rpc_url)
        .read_current_record(
            vera_domain::ModuleId::Acp,
            &vera_modules::acp::keys::policy_key(policy_id),
            1,
            keys.epoch_info().output.public().public(),
            vera_client::RECORD_PROOF_BYTES,
        )
        .await
        .expect("verified policy record");
    response.record.value.is_some()
}
