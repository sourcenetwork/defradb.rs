//! Actors of each key type, against a Vera node hosted in the test process.

// The layout of the node's start future is deeper than the default limit.
#![recursion_limit = "256"]

use std::path::Path;
use std::sync::Arc;

use commonware_codec::Encode as _;
use serde_json::json;
use vera::vera_rs::{
    DelegationSigner, Ed25519Signer, NativeWorker, P256Signer, Secp256k1Signer, VeraRsProvider,
};
use vera::{AcpTuning, SubjectRef, VeraProvider};
use vera_embedded::EmbeddedTransport;
use vera_node::{EmbeddedConfig, EmbeddedNode, SoloGenesis};

const DEPLOYMENT: u64 = 9405;
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const POLICY: &str = "\
name: actors
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
    EmbeddedNode::start(config).await.expect("embedded node")
}

/// A provider acting as `signer`'s actor, with a worker journal of its own.
async fn provider(
    node: &EmbeddedNode,
    directory: &Path,
    signer: Arc<dyn DelegationSigner>,
) -> VeraRsProvider {
    let tuning = AcpTuning::default();
    let edge = edge::testing::edge();
    let transport = Arc::new(EmbeddedTransport::new(
        node.rpc().clone(),
        tuning.request_timeout,
        &edge,
    ));
    let keyring =
        keyring::FileKeyring::open(directory.join("keys"), b"test-password").expect("keyring");
    let worker =
        NativeWorker::open(&directory.join("worker"), &keyring, DEPLOYMENT).expect("worker");
    VeraRsProvider::with_signer(
        transport.clone(),
        transport,
        &hex::encode(node.consensus_key().encode()),
        signer,
        worker,
        &tuning,
        None,
        &edge,
    )
    .await
    .expect("provider")
}

#[nuclei::test]
async fn an_actor_of_each_key_type_owns_grants_and_reads_through_vera() {
    let directory = tempfile::tempdir().unwrap();
    let node = start(&directory.path().join("vera")).await;

    let owners: [(&str, Arc<dyn DelegationSigner>); 3] = [
        (
            "secp256k1",
            Arc::new(Secp256k1Signer::from_slice(&[42; 32]).expect("key")),
        ),
        (
            "ed25519",
            Arc::new(Ed25519Signer::from_slice(&[42; 32]).expect("key")),
        ),
        (
            "p256",
            Arc::new(P256Signer::from_slice(&[42; 32]).expect("key")),
        ),
    ];
    // Readers named the way DefraDB names them: a P-256 reader by its
    // uncompressed key, which vera.rs does not take as written.
    let readers = [
        Secp256k1Signer::from_slice(&[43; 32])
            .expect("key")
            .did()
            .to_owned(),
        Ed25519Signer::from_slice(&[43; 32])
            .expect("key")
            .did()
            .to_owned(),
        P256Signer::from_slice(&[43; 32])
            .expect("key")
            .did()
            .to_owned(),
    ];

    for (kind, signer) in owners {
        let owner = signer.did().to_owned();
        let provider = provider(&node, &directory.path().join(kind), signer).await;
        // The provider names its actor as DefraDB does.
        assert_eq!(
            provider.self_did().as_deref(),
            Some(owner.as_str()),
            "{kind}"
        );

        let policy = provider.create_policy(POLICY).await.expect("policy");
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
            (true, owner.clone()),
            "{kind}: the owner vera.rs records is read back in DefraDB's spelling"
        );
        assert!(
            provider
                .verify_access(&policy, "document", "first", "read", &owner)
                .await
                .expect("permission proof"),
            "{kind}: the owner reads"
        );

        for reader in &readers {
            assert!(!provider
                .verify_access(&policy, "document", "first", "read", reader)
                .await
                .expect("permission proof before the grant"));
            provider
                .set_relationship(
                    &delegation,
                    &policy,
                    "document",
                    "first",
                    "reader",
                    &SubjectRef::Actor(reader.clone()),
                )
                .await
                .expect("grant");
            assert!(
                provider
                    .verify_access(&policy, "document", "first", "read", reader)
                    .await
                    .expect("permission proof after the grant"),
                "{kind}: a grant to {reader} admits it"
            );
        }
        drop(provider);
    }

    node.stop().await.expect("stop");
}

/// A grant made through an entity subject names its actor as an access check
/// does: a P-256 reader granted that way is admitted.
#[nuclei::test]
async fn a_p256_entity_subject_grant_admits_the_reader() {
    let directory = tempfile::tempdir().unwrap();
    let node = start(&directory.path().join("vera")).await;
    let owner: Arc<dyn DelegationSigner> =
        Arc::new(Secp256k1Signer::from_slice(&[42; 32]).expect("key"));
    let owner_did = owner.did().to_owned();
    let provider = provider(&node, &directory.path().join("owner"), owner).await;
    let policy = provider.create_policy(POLICY).await.expect("policy");
    let delegation = provider
        .create_bearer_token(&owner_did)
        .await
        .expect("delegation");
    provider
        .register_object(&delegation, &policy, "document", "first")
        .await
        .expect("registration");

    let reader = P256Signer::from_slice(&[43; 32])
        .expect("key")
        .did()
        .to_owned();
    provider
        .set_relationship_subject(&policy, "document", "first", "reader", 0, "", &reader, "")
        .await
        .expect("grant through an entity subject");
    assert!(
        provider
            .verify_access(&policy, "document", "first", "read", &reader)
            .await
            .expect("permission proof after the grant"),
        "an entity-subject grant to {reader} admits it"
    );

    node.stop().await.expect("stop");
}
