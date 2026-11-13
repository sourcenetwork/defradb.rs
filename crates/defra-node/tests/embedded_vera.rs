//! A node that runs its own Vera node for document ACP.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use acp::{DocumentPermission, Identity};
use defra_node::{EmbeddedVeraConfig, EmbeddedVeraDeployment, Node};

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
            },
            listen: "127.0.0.1:0".parse().expect("address"),
            start_deadline: Duration::from_secs(60),
            stop_deadline: Duration::from_secs(30),
            actor_key: vec![42; 32],
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

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}
