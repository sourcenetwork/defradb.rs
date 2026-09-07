use std::time::Duration;

use commonware_codec::Encode as _;

use identity::{Identity, IdentityKeyType, RawIdentity};
use integration_test::{BinarySource, TestCluster, TestIdentity};

pub use integration_test::sourcehub_cli_binary as defra_binary;

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

/// Start a 1-node hub.rs devnet cluster and wait for it to be healthy.
pub async fn start_hub_cluster() -> hub_harness::cluster::TestCluster {
    let cluster = hub_harness::cluster::TestCluster::builder()
        .nodes(1)
        .seed(0)
        .build()
        .await
        .expect("start hub.rs cluster");
    cluster
        .wait_ready(Duration::from_secs(30))
        .await
        .expect("hub.rs cluster ready");
    cluster
}

/// Build a DefraDB test cluster configured to use hub.rs for document ACP.
///
/// Sets `DEFRA_HUB_RS_ADDRESS` and `DEFRA_ACP_DOCUMENT_TYPE` env vars before
/// spawning nodes (the CLI reads these at startup). Env vars are cleared after
/// the cluster is built so they don't leak to other tests.
pub async fn build_defra_with_hub_rs(
    hub_rpc_url: &str,
    identity: &str,
    n_nodes: usize,
    p2p: bool,
) -> TestCluster {
    let previous_address = std::env::var_os("DEFRA_HUB_RS_ADDRESS");
    let keys = hub_harness::cluster::KeySet::builder()
        .nodes(1)
        .seed(0)
        .build()
        .expect("bootstrap keys");
    let trusted_key = hex::encode(keys.epoch_info().output.public().public().encode());
    let previous_key = std::env::var_os("DEFRA_VERA_CONSENSUS_KEY");
    let previous_deployment = std::env::var_os("DEFRA_VERA_DEPLOYMENT_ID");
    let previous_type = std::env::var_os("DEFRA_ACP_DOCUMENT_TYPE");
    unsafe {
        std::env::set_var("DEFRA_HUB_RS_ADDRESS", hub_rpc_url);
        std::env::set_var("DEFRA_VERA_CONSENSUS_KEY", trusted_key);
        std::env::set_var("DEFRA_VERA_DEPLOYMENT_ID", "9001");
        std::env::set_var("DEFRA_ACP_DOCUMENT_TYPE", "hub-rs");
    }

    let mut builder = TestCluster::builder()
        .rust_nodes(n_nodes)
        .with_keyring()
        .with_rust_binary(BinarySource::Path(defra_binary()));
    for index in 0..n_nodes {
        builder = builder.with_node_identity(index, identity.to_string());
    }
    if p2p {
        builder = builder.with_p2p();
    }
    let cluster = builder.build().await;

    unsafe {
        for (name, value) in [
            ("DEFRA_HUB_RS_ADDRESS", previous_address),
            ("DEFRA_VERA_CONSENSUS_KEY", previous_key),
            ("DEFRA_VERA_DEPLOYMENT_ID", previous_deployment),
            ("DEFRA_ACP_DOCUMENT_TYPE", previous_type),
        ] {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
    cluster.expect("build defra cluster")
}

pub async fn policy_exists(hub_rpc_url: &str, policy_id: &str) -> bool {
    let keys = hub_harness::cluster::KeySet::builder()
        .nodes(1)
        .seed(0)
        .build()
        .expect("bootstrap keys");
    let response = hub_client::HubClient::new(hub_rpc_url)
        .read_current_record(
            hub_domain::ModuleId::Acp,
            &hub_modules::acp::keys::policy_key(policy_id),
            1,
            keys.epoch_info().output.public().public(),
            hub_client::RECORD_PROOF_BYTES,
        )
        .await
        .expect("verified policy record");
    response.record.value.is_some()
}
