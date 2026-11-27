//! Policy IDs that vera.rs derives from the definition, the creator and a
//! nonce, against a Vera node hosted in the test process.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use vera::vera_rs::{DelegationSigner, Ed25519Signer, NativeWorker, P256Signer, VeraRsProvider};
use vera::{AcpTuning, PolicyNonce, VeraProvider};
use vera_client::PolicyMarshalingType;
use vera_embedded::{Deployment, HostConfig, HostedNode, SeededPolicy};

const DEPLOYMENT: u64 = 9406;
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const POLICY: &str = "\
name: ids
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

async fn start(directory: &Path, policies: Vec<SeededPolicy>) -> HostedNode {
    let config = HostConfig {
        data_dir: directory.join("vera"),
        deployment: Deployment {
            chain_id: DEPLOYMENT,
            threshold: 1,
            operator_keys: vec![ACCOUNT_KEY.to_string()],
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_secs(),
            policies,
        },
        device: vera_embedded::HostDevice::Alone,
        listen: "127.0.0.1:0".parse().expect("address"),
        start_deadline: Duration::from_secs(60),
        stop_deadline: Duration::from_secs(30),
    };
    HostedNode::start(config, &edge::testing::edge())
        .await
        .expect("hosted node")
}

/// A provider acting as `signer`'s actor, with a worker journal of its own
/// under `name`.
async fn provider(
    node: &HostedNode,
    directory: &Path,
    name: &str,
    signer: Arc<dyn DelegationSigner>,
) -> VeraRsProvider {
    let tuning = AcpTuning::default();
    let transport = Arc::new(node.transport(tuning.request_timeout));
    let keyring = keyring::FileKeyring::open(directory.join(name).join("keys"), b"test-password")
        .expect("keyring");
    let worker = NativeWorker::open(&directory.join(name).join("worker"), &keyring, DEPLOYMENT)
        .expect("worker");
    VeraRsProvider::with_signer(
        transport.clone(),
        transport,
        node.consensus_key(),
        vera::vera_rs::GenesisSource::Hosted,
        signer,
        worker,
        &tuning,
        None,
        &edge::testing::edge(),
    )
    .await
    .expect("provider")
}

fn ed25519(seed: u8) -> Arc<dyn DelegationSigner> {
    Arc::new(Ed25519Signer::from_slice(&[seed; 32]).expect("key"))
}

async fn stored(provider: &VeraRsProvider, id: &str) -> Option<String> {
    provider
        .query_policy(id)
        .await
        .expect("policy read")
        .and_then(|policy| policy.raw_policy)
}

/// With its own nonce a caller knows the ID before the policy exists: the
/// one vera.rs's own derivation gives for the definition, nonce and actor.
#[nuclei::test]
async fn a_supplied_nonce_gives_the_id_known_beforehand() {
    let directory = tempfile::tempdir().unwrap();
    let node = start(directory.path(), Vec::new()).await;
    let nonce = PolicyNonce([3; 32]);

    let owner = provider(&node, directory.path(), "ed25519", ed25519(42)).await;
    let expected = vera_client::predict_policy_id(
        POLICY,
        &PolicyMarshalingType::ShortYaml,
        &vera_client::PolicyNonce(nonce.0),
        &owner.self_did().expect("actor DID"),
    )
    .unwrap();
    assert_eq!(owner.predict_policy_id(POLICY, nonce).unwrap(), expected);
    let id = owner.create_policy(POLICY, Some(nonce)).await.unwrap();
    assert_eq!(id, expected);
    assert_eq!(stored(&owner, &id).await.as_deref(), Some(POLICY));

    // A P-256 actor is recorded under the one spelling vera.rs takes; the
    // ID known beforehand is the one it gets all the same.
    let signer: Arc<dyn DelegationSigner> =
        Arc::new(P256Signer::from_slice(&[43; 32]).expect("key"));
    let p256 = provider(&node, directory.path(), "p256", signer).await;
    let known = p256.predict_policy_id(POLICY, nonce).unwrap();
    assert_eq!(
        p256.create_policy(POLICY, Some(nonce)).await.unwrap(),
        known
    );
    node.stop().await.expect("stop");
}

/// One actor creating one definition twice under two nonces makes two
/// policies, and so does a caller that leaves the nonce to the provider.
#[nuclei::test]
async fn two_creates_with_two_nonces_make_two_policies() {
    let directory = tempfile::tempdir().unwrap();
    let node = start(directory.path(), Vec::new()).await;
    let owner = provider(&node, directory.path(), "owner", ed25519(42)).await;

    let first = owner
        .create_policy(POLICY, Some(PolicyNonce([1; 32])))
        .await
        .unwrap();
    let second = owner
        .create_policy(POLICY, Some(PolicyNonce([2; 32])))
        .await
        .unwrap();
    assert_ne!(first, second);
    let drawn = owner.create_policy(POLICY, None).await.unwrap();
    let drawn_again = owner.create_policy(POLICY, None).await.unwrap();
    assert_ne!(drawn, drawn_again);
    for id in [&first, &second, &drawn, &drawn_again] {
        assert_eq!(stored(&owner, id).await.as_deref(), Some(POLICY), "{id}");
    }
    node.stop().await.expect("stop");
}

/// A create sent again with its nonce, as after an answer that was lost,
/// returns the policy the first one made and makes no second one.
#[nuclei::test]
async fn a_create_sent_again_with_its_nonce_returns_the_same_policy() {
    let directory = tempfile::tempdir().unwrap();
    let node = start(directory.path(), Vec::new()).await;
    let owner = provider(&node, directory.path(), "owner", ed25519(42)).await;
    let nonce = PolicyNonce([9; 32]);

    let first = owner.create_policy(POLICY, Some(nonce)).await.unwrap();
    let again = owner.create_policy(POLICY, Some(nonce)).await.unwrap();
    assert_eq!(again, first);
    assert_eq!(stored(&owner, &first).await.as_deref(), Some(POLICY));
    node.stop().await.expect("stop");
}

/// The creator is part of the ID: two actors creating one definition under
/// one nonce get two policies, and neither blocks the other.
#[nuclei::test]
async fn one_definition_by_two_creators_gets_two_ids() {
    let directory = tempfile::tempdir().unwrap();
    let node = start(directory.path(), Vec::new()).await;
    let alice = provider(&node, directory.path(), "alice", ed25519(42)).await;
    let bob = provider(&node, directory.path(), "bob", ed25519(43)).await;
    let nonce = PolicyNonce::default();

    let hers = alice.create_policy(POLICY, Some(nonce)).await.unwrap();
    let his = bob.create_policy(POLICY, Some(nonce)).await.unwrap();
    assert_ne!(hers, his);
    assert_eq!(stored(&alice, &hers).await.as_deref(), Some(POLICY));
    assert_eq!(stored(&bob, &his).await.as_deref(), Some(POLICY));
    node.stop().await.expect("stop");
}

/// A deployment seeded with a policy holds it under the ID the host computed
/// beforehand; its creator's own create under the same nonce returns it, and
/// another actor's create of the definition gets a policy of its own.
#[nuclei::test]
async fn a_seeded_policy_is_found_under_its_predicted_id() {
    let directory = tempfile::tempdir().unwrap();
    let home = ed25519(42);
    let seed = SeededPolicy {
        definition: POLICY.to_string(),
        nonce: PolicyNonce::default(),
        creator: home.did().to_owned(),
    };
    let seeded = seed.id().unwrap();
    let node = start(directory.path(), vec![seed.clone()]).await;
    let owner = provider(&node, directory.path(), "owner", home).await;

    assert_eq!(stored(&owner, &seeded).await.as_deref(), Some(POLICY));
    assert_eq!(
        owner.create_policy(POLICY, Some(seed.nonce)).await.unwrap(),
        seeded
    );
    let stranger = provider(&node, directory.path(), "stranger", ed25519(43)).await;
    let theirs = stranger
        .create_policy(POLICY, Some(seed.nonce))
        .await
        .unwrap();
    assert_ne!(theirs, seeded);
    node.stop().await.expect("stop");
}
