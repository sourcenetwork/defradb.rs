use std::sync::Arc;

use acp::{DocumentPermission, Identity};
use commonware_codec::Encode as _;
use integration_test::USER_ACP_POLICY;

use super::helpers;

#[nuclei::test]
#[serial_test::serial]
async fn node_built_with_vera_rs_enforces_verified_document_access() {
    edge::compat(async {
        let hub = helpers::start_hub_cluster().await;
        let keys = vera_harness::cluster::KeySet::builder()
            .nodes(1)
            .seed(0)
            .build()
            .expect("bootstrap keys");
        let consensus_key = hex::encode(keys.epoch_info().output.public().public().encode());
        let directory = tempfile::tempdir().unwrap();
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(directory.path().join("keys"), b"test-password").unwrap(),
        );
        let owner = helpers::funded_identity();
        let node = defra_node::Node::builder()
            .with_vera_rs(defra_node::VeraRsConfig {
                rpc_address: hub.node(0).rpc_url(),
                consensus_key,
                deployment_id: 9001,
                actor_key: hex::decode(&owner.private_key_hex).expect("owner key"),
                worker_dir: directory.path().join("worker"),
                keyring,
                tuning: vera::AcpTuning::default(),
            })
            .build()
            .await
            .expect("node");

        exercise(&node, &hub.node(0).rpc_url()).await;
        node.shutdown().await.expect("shutdown");
    })
    .await;
}

#[nuclei::test]
#[serial_test::serial]
async fn node_built_with_a_host_provider_enforces_verified_document_access() {
    edge::compat(async {
        let hub = helpers::start_hub_cluster().await;
        let keys = vera_harness::cluster::KeySet::builder()
            .nodes(1)
            .seed(0)
            .build()
            .expect("bootstrap keys");
        let consensus_key = hex::encode(keys.epoch_info().output.public().public().encode());
        let directory = tempfile::tempdir().unwrap();
        let keyring =
            keyring::FileKeyring::open(directory.path().join("keys"), b"test-password").unwrap();
        let worker =
            vera::vera_rs::NativeWorker::open(&directory.path().join("worker"), &keyring, 9001)
                .unwrap();
        let owner = helpers::funded_identity();
        let provider: Arc<dyn vera::VeraProvider> = Arc::new(
            vera::VeraRsProvider::new(
                hub.node(0).rpc_url(),
                &consensus_key,
                &hex::decode(&owner.private_key_hex).expect("owner key"),
                worker,
                &vera::AcpTuning::default(),
                None,
                &edge::testing::edge(),
            )
            .await
            .expect("provider"),
        );
        let node = defra_node::Node::builder()
            .with_document_acp_provider(defra_node::DocumentAcpProvider::new(provider))
            .build()
            .await
            .expect("node");

        exercise(&node, &hub.node(0).rpc_url()).await;
        node.shutdown().await.expect("shutdown");
    })
    .await;
}

/// A policy created, a document registered, its owner admitted and a stranger
/// refused, all through the node's document ACP.
async fn exercise(node: &defra_node::Node, rpc_url: &str) {
    let vera = node.vera_acp().expect("vera.rs document ACP").clone();
    let owner_did = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
    let policy = vera
        .add_policy(owner_did.as_str(), USER_ACP_POLICY)
        .await
        .expect("policy");
    assert!(helpers::policy_exists(rpc_url, &policy).await);

    let document_acp = node.document_acp();
    document_acp
        .register_doc_object(&owner_did, &policy, "users", "node-document")
        .await
        .expect("register document");
    assert!(document_acp
        .check_doc_access(
            &Identity::Authenticated(owner_did.clone()),
            DocumentPermission::Read,
            &policy,
            "users",
            "node-document",
        )
        .await
        .expect("owner access"));
    let stranger =
        identity::Did::new("did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR").unwrap();
    assert!(!document_acp
        .check_doc_access(
            &Identity::Authenticated(stranger),
            DocumentPermission::Read,
            &policy,
            "users",
            "node-document",
        )
        .await
        .expect("stranger access"));
}
