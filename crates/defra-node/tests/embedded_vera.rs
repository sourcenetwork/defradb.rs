//! A node that runs its own Vera node for document ACP.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use acp::{DocumentPermission, Identity};
use defra_node::{
    EmbeddedVeraActor, EmbeddedVeraConfig, EmbeddedVeraDeployment, EmbeddedVeraPolicy, Node,
};

/// The compressed secp256k1 key of the account that administers the deployment.
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const STRANGER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const POLICY: &str = "\
name: embedded
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

async fn node(directory: &Path) -> Node {
    node_as(directory, EmbeddedVeraActor::Secp256k1Key(vec![42; 32])).await
}

async fn node_as(directory: &Path, actor: EmbeddedVeraActor) -> Node {
    node_with(directory, actor, Vec::new()).await
}

async fn node_with(
    directory: &Path,
    actor: EmbeddedVeraActor,
    policies: Vec<EmbeddedVeraPolicy>,
) -> Node {
    node_serving(directory, actor, policies, None).await
}

async fn node_serving(
    directory: &Path,
    actor: EmbeddedVeraActor,
    policies: Vec<EmbeddedVeraPolicy>,
    rpc_listen: Option<std::net::SocketAddr>,
) -> Node {
    let keyring: Arc<dyn keyring::Keyring> = Arc::new(
        keyring::FileKeyring::open(directory.join("keys"), b"test-password").expect("keyring"),
    );
    Node::builder()
        .with_embedded_vera(EmbeddedVeraConfig {
            data_dir: directory.join("vera"),
            deployment: EmbeddedVeraDeployment {
                chain_id: 9403,
                threshold: 1,
                operator_keys: vec![ACCOUNT_KEY.to_string()],
                created: now(),
                policies,
            },
            listen: "127.0.0.1:0".parse().expect("address"),
            start_deadline: Duration::from_secs(60),
            stop_deadline: Duration::from_secs(30),
            actor,
            rpc_listen,
            worker_dir: directory.join("worker"),
            keyring,
            tuning: vera::AcpTuning::default(),
        })
        .build()
        .await
        .expect("node")
}

async fn reads(node: &Node, actor: &identity::Did, policy: &str) -> bool {
    node.document_acp()
        .check_doc_access(
            &Identity::Authenticated(actor.clone()),
            DocumentPermission::Read,
            policy,
            "users",
            "first",
        )
        .await
        .expect("access decision")
}

#[test]
fn a_node_decides_document_access_through_its_own_vera_node_across_a_restart() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let stranger = identity::Did::new(STRANGER).unwrap();

        let first = node(directory.path()).await;
        let vera = first.vera_acp().expect("vera.rs document ACP").clone();
        let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let policy = vera
            .add_policy(owner.as_str(), POLICY)
            .await
            .expect("policy");
        first
            .document_acp()
            .register_doc_object(&owner, &policy, "users", "first")
            .await
            .expect("registration");
        assert!(reads(&first, &owner, &policy).await);
        assert!(!reads(&first, &stranger, &policy).await);
        drop(vera);
        first.shutdown().await.expect("shutdown");
        // Nothing answers for the Vera node once it has stopped.
        assert!(first
            .document_acp()
            .check_doc_access(
                &Identity::Authenticated(owner.clone()),
                DocumentPermission::Read,
                &policy,
                "users",
                "first",
            )
            .await
            .is_err());
        // A node holds its worker journal until it is dropped.
        drop(first);

        // The deployment, the policy and the registration are the Vera
        // node's: a second node over the same directory finds them.
        let second = node(directory.path()).await;
        assert!(reads(&second, &owner, &policy).await);
        assert!(!reads(&second, &stranger, &policy).await);
        second.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// The actor may be a key the node never sees: a signer for an Ed25519 or a
/// P-256 identity owns the node's policies under that identity's DID.
#[test]
fn a_node_acts_for_a_signer_of_another_key_type() {
    use vera::vera_rs::{DelegationSigner, Ed25519Signer, P256Signer};

    workers::block_on(async {
        let signers: [Arc<dyn DelegationSigner>; 2] = [
            Arc::new(Ed25519Signer::from_slice(&[42; 32]).expect("key")),
            Arc::new(P256Signer::from_slice(&[42; 32]).expect("key")),
        ];
        for signer in signers {
            let directory = tempfile::tempdir().unwrap();
            let actor = signer.did().to_owned();
            let node = node_as(directory.path(), EmbeddedVeraActor::Signer(signer)).await;
            let vera = node.vera_acp().expect("vera.rs document ACP").clone();
            assert_eq!(vera.self_did().as_deref(), Some(actor.as_str()));

            let owner = identity::Did::new(&actor).unwrap();
            let policy = vera
                .add_policy(owner.as_str(), POLICY)
                .await
                .expect("policy");
            node.document_acp()
                .register_doc_object(&owner, &policy, "users", "first")
                .await
                .expect("registration");
            assert!(
                reads(&node, &owner, &policy).await,
                "{actor} reads its document"
            );
            let stranger = identity::Did::new(STRANGER).unwrap();
            assert!(!reads(&node, &stranger, &policy).await);
            drop(vera);
            node.shutdown().await.expect("shutdown");
        }
    })
    .unwrap();
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

/// A host that already names its policy by the ID a local ACP derives finds
/// it under that ID on its own Vera node, across a restart.
#[test]
fn a_node_adopts_a_policy_its_vera_node_was_seeded_with() {
    use vera::vera_rs::{DelegationSigner, Ed25519Signer};

    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[42; 32]).expect("key"));
        let owner = identity::Did::new(signer.did()).unwrap();
        let stranger = identity::Did::new(STRANGER).unwrap();
        let context = defra_core::RequestContext::default().with_identity(Some(owner.to_string()));
        let local = acp::policy_yaml::build_policy(
            &acp::policy_yaml::parse_policy_yaml(POLICY).unwrap(),
            0,
        )
        .unwrap()
        .id;
        let seeded = || {
            vec![EmbeddedVeraPolicy {
                definition: POLICY.to_string(),
                counter: 0,
                owner: owner.to_string(),
            }]
        };

        let first = node_with(
            directory.path(),
            EmbeddedVeraActor::Signer(Arc::clone(&signer)),
            seeded(),
        )
        .await;
        let adopted = first
            .add_dac_policy_with_counter_as(&context, POLICY, 0)
            .await
            .expect("seeded policy");
        assert_eq!(adopted, local);
        assert!(first
            .add_dac_policy_with_counter_as(&context, POLICY, 1)
            .await
            .is_err());
        first
            .document_acp()
            .register_doc_object(&owner, &adopted, "users", "first")
            .await
            .expect("registration");
        assert!(reads(&first, &owner, &adopted).await);
        assert!(!reads(&first, &stranger, &adopted).await);
        first.shutdown().await.expect("shutdown");
        drop(first);

        let second = node_with(
            directory.path(),
            EmbeddedVeraActor::Signer(signer),
            seeded(),
        )
        .await;
        assert_eq!(
            second
                .add_dac_policy_with_counter_as(&context, POLICY, 0)
                .await
                .expect("seeded policy"),
            local
        );
        assert!(reads(&second, &owner, &adopted).await);
        second.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// Two nodes of one account share one deployment: the first runs the Vera
/// node, the second verifies against it over its RPC listener and acts as
/// its own actor. What either writes, the other reads.
#[test]
fn a_second_node_shares_the_first_nodes_vera_deployment() {
    use defra_node::{VeraRsActor, VeraRsConfig};
    use vera::vera_rs::{DelegationSigner, Ed25519Signer};

    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let first_signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[42; 32]).expect("key"));
        let second_signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[43; 32]).expect("key"));
        let owner = identity::Did::new(first_signer.did()).unwrap();
        let peer = identity::Did::new(second_signer.did()).unwrap();
        let rpc = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();

        let first = node_serving(
            &directory.path().join("first"),
            EmbeddedVeraActor::Signer(first_signer),
            vec![EmbeddedVeraPolicy {
                definition: POLICY.to_string(),
                counter: 0,
                owner: owner.to_string(),
            }],
            Some(rpc),
        )
        .await;
        let trust = first.embedded_vera_trust().expect("deployment").clone();

        let second_dir = directory.path().join("second");
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(second_dir.join("keys"), b"test-password").expect("keyring"),
        );
        let second = Node::builder()
            .with_vera_rs(VeraRsConfig {
                rpc_address: format!("http://{rpc}"),
                consensus_key: trust.consensus_key,
                deployment_id: trust.chain_id,
                actor: VeraRsActor::Signer(second_signer),
                worker_dir: second_dir.join("worker"),
                keyring,
                tuning: vera::AcpTuning::default(),
            })
            .build()
            .await
            .expect("second node");

        let policy = first
            .add_dac_policy_with_counter_as(
                &defra_core::RequestContext::default().with_identity(Some(owner.to_string())),
                POLICY,
                0,
            )
            .await
            .expect("seeded policy");
        assert_eq!(
            second
                .add_dac_policy_with_counter_as(
                    &defra_core::RequestContext::default().with_identity(Some(peer.to_string())),
                    POLICY,
                    0,
                )
                .await
                .expect("seeded policy"),
            policy
        );

        first
            .document_acp()
            .register_doc_object(&owner, &policy, "users", "first")
            .await
            .expect("registration");
        assert!(reads(&second, &owner, &policy).await);
        assert!(!reads(&second, &peer, &policy).await);

        second
            .document_acp()
            .register_doc_object(&peer, &policy, "users", "second")
            .await
            .expect("registration by the second node");
        let seen = first
            .document_acp()
            .check_doc_access(
                &Identity::Authenticated(peer.clone()),
                DocumentPermission::Read,
                &policy,
                "users",
                "second",
            )
            .await
            .expect("access decision");
        assert!(seen);

        second.shutdown().await.expect("shutdown");
        first.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
