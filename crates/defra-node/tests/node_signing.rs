//! Every write a signing node makes is signed: by the request identity when it
//! is registered locally, by the node identity otherwise.

use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_core::RequestContext;
use defra_node::{Node, NodeBuilder, SigningKey};
use identity::Identity as _;

const NODE_KEY: [u8; 32] = [0x11; 32];
const USER_KEY: [u8; 32] = [0x22; 32];
const ADD_USER: &str = r#"mutation { add_Users(input: {name: "Alice"}) { _docID } }"#;

fn did_of(key: &[u8]) -> String {
    raw_identity(key).did().expect("DID derives").to_string()
}

fn raw_identity(key: &[u8]) -> identity::RawIdentity {
    let private_key = crypto::Secp256k1PrivateKey::from_bytes(key).expect("valid secp256k1 key");
    identity::RawIdentity::from_secp256k1(private_key).expect("identity from key")
}

/// Registers the identity of `key` locally, the way an embedding application
/// registers a user it signs for.
fn register_local_identity(key: &[u8]) -> String {
    let identity = raw_identity(key);
    let did = identity.did().expect("DID derives").to_string();
    defra_core::signing::store_identity(
        &did,
        SigningConfig {
            key_type: SigningKeyType::Secp256k1,
            private_key_bytes: SigningConfig::private_key_bytes_from_vec(
                identity.private_key_bytes(),
            ),
            public_key_bytes: identity.public_key_bytes().to_vec(),
            public_key_hex: hex::encode(identity.public_key_bytes()),
            remote_signer: None,
            signing_authorization: None,
        },
    );
    did
}

async fn node_with_users(builder: NodeBuilder) -> Node {
    let node = builder.build().await.expect("node starts");
    node.add_collection("type Users { name: String }")
        .await
        .expect("collection added");
    node
}

async fn commit_cids(node: &Node) -> Vec<String> {
    let response = node.execute("query { _commits { cid } }").await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let cids: Vec<String> = response
        .data
        .as_ref()
        .and_then(|data| data["_commits"].as_array())
        .expect("_commits array")
        .iter()
        .map(|commit| commit["cid"].as_str().expect("commit cid").to_owned())
        .collect();
    assert!(!cids.is_empty(), "expected commits for the write");
    cids
}

/// The DID that verifiably signed each commit, `None` for an unsigned one.
async fn commit_signers(node: &Node) -> Vec<Option<String>> {
    let mut signers = Vec::new();
    for cid in commit_cids(node).await {
        signers.push(node.verified_block_signer_did(&cid).await.ok());
    }
    signers
}

async fn assert_all_signed_by(node: &Node, expected: &str) {
    let signers = commit_signers(node).await;
    assert!(
        signers
            .iter()
            .all(|signer| signer.as_deref() == Some(expected)),
        "expected every commit signed by {expected}, got {signers:?}"
    );
}

#[test]
fn execute_signs_with_the_node_identity() {
    workers::block_on(async {
        let node = node_with_users(
            Node::builder().with_signing_key(SigningKey::Secp256k1(NODE_KEY.to_vec())),
        )
        .await;

        let created = node.execute(ADD_USER).await;
        assert!(!created.has_errors(), "{:?}", created.errors);

        assert_all_signed_by(&node, &did_of(&NODE_KEY)).await;
        node.shutdown().await.expect("node shuts down");
    })
    .unwrap()
}

#[test]
fn execute_as_signs_with_the_node_identity() {
    workers::block_on(async {
        let node = node_with_users(
            Node::builder().with_signing_key(SigningKey::Secp256k1(NODE_KEY.to_vec())),
        )
        .await;

        let created = node.execute_as(&RequestContext::default(), ADD_USER).await;
        assert!(!created.has_errors(), "{:?}", created.errors);

        assert_all_signed_by(&node, &did_of(&NODE_KEY)).await;
        node.shutdown().await.expect("node shuts down");
    })
    .unwrap()
}

#[test]
fn execute_as_signs_with_a_locally_registered_request_identity() {
    workers::block_on(async {
        let node = node_with_users(
            Node::builder().with_signing_key(SigningKey::Secp256k1(NODE_KEY.to_vec())),
        )
        .await;
        let user_did = register_local_identity(&USER_KEY);

        let context = RequestContext::default().with_identity(Some(user_did.clone()));
        let created = node.execute_as(&context, ADD_USER).await;
        assert!(!created.has_errors(), "{:?}", created.errors);

        assert_all_signed_by(&node, &user_did).await;
        node.shutdown().await.expect("node shuts down");
    })
    .unwrap()
}

#[test]
fn execute_as_leaves_writes_unsigned_without_signing() {
    workers::block_on(async {
        let node = node_with_users(Node::builder()).await;

        let created = node.execute_as(&RequestContext::default(), ADD_USER).await;
        assert!(!created.has_errors(), "{:?}", created.errors);

        let signers = commit_signers(&node).await;
        assert!(
            signers.iter().all(Option::is_none),
            "expected unsigned commits, got {signers:?}"
        );
        node.shutdown().await.expect("node shuts down");
    })
    .unwrap()
}
