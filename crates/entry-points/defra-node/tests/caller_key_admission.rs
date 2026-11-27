//! Every `Node` entry that takes a caller's context admits the caller's key
//! before anything is written: the key must be a usable local Ed25519 or
//! secp256k1 key of the actor the context names, with DAC left on. A refusal
//! writes nothing, and a request that names an identity but brings no key
//! signs with the node's key, never with a key the process holds for that
//! identity.

use crypto::Key;
use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_node::{Node, ScopedNode};
use identity::Did;
use identity::{Identity as _, RawIdentity};

const CREATE: &str = r#"mutation { add_Fact(input: {value: "one"}) { _docID } }"#;
const BACKUP: &str = r#"{"Fact":[{"value":"imp"}]}"#;
const MISMATCH: &str = "explicit signing key must match the selected actor";
const BYPASS: &str = "explicit embedded signing does not allow DAC bypass";
const UNSUPPORTED: &str =
    "embedded signing requires a local Ed25519 or secp256k1 key without a remote signer";

fn config(key_type: SigningKeyType, private: Vec<u8>, identity: &RawIdentity) -> SigningConfig {
    let public = identity.public_key_bytes();
    SigningConfig {
        key_type,
        private_key_bytes: private,
        public_key_hex: hex::encode(&public),
        public_key_bytes: public,
        remote_signer: None,
        signing_authorization: None,
    }
}

fn ed_key() -> (String, SigningConfig) {
    let private = crypto::generate_ed25519().unwrap();
    let identity = RawIdentity::from_ed25519(private.clone()).unwrap();
    let config = config(SigningKeyType::Ed25519, private.raw().to_vec(), &identity);
    (identity.did().unwrap().to_string(), config)
}

fn k1_key() -> (String, SigningConfig) {
    let private = crypto::generate_secp256k1().unwrap();
    let identity = RawIdentity::from_secp256k1(private.clone()).unwrap();
    let config = config(SigningKeyType::Secp256k1, private.raw().to_vec(), &identity);
    (identity.did().unwrap().to_string(), config)
}

/// A node with an Ed25519 node identity that signs, and a `Fact` collection.
async fn node() -> (Node, String) {
    let (node_did, node_key) = ed_key();
    defra_core::signing::store_identity(&node_did, node_key).unwrap();
    let node = Node::builder()
        .with_node_identity_did(&node_did)
        .build()
        .await
        .unwrap();
    node.add_collection("type Fact { value: String }")
        .await
        .unwrap();
    (node, node_did)
}

fn as_actor<'n>(node: &'n Node, actor: &str, key: Option<SigningConfig>) -> ScopedNode<'n> {
    node.with_identity(Did::new(actor).unwrap())
        .with_signing(key)
}

async fn fact_count(node: &Node) -> usize {
    let rows = node.execute("{ Fact { value } }").await;
    assert!(!rows.has_errors(), "{:?}", rows.errors);
    rows.data.unwrap()["Fact"].as_array().unwrap().len()
}

/// The DID whose signature `_C` of the only `Fact` carries.
async fn composite_signer(node: &Node) -> String {
    let rows = node.execute("{ Fact { _docID } }").await;
    let doc = rows.data.unwrap()["Fact"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    let commits = node
        .execute(&format!(
            r#"{{ _commits(docID: "{doc}", filter: {{fieldName: {{_eq: "_C"}}}}) {{ cid }} }}"#
        ))
        .await;
    let cid = commits.data.unwrap()["_commits"][0]["cid"]
        .as_str()
        .unwrap()
        .to_owned();
    node.verified_block_signer_did(&cid).await.unwrap()
}

fn refusal(response: &query::QueryResponse) -> String {
    assert!(response.has_errors(), "the request must be refused");
    format!("{:?}", response.errors)
}

#[test]
fn execute_refuses_a_key_of_another_actor() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (_signer, signer_key) = k1_key();
        let (actor, _) = k1_key();
        let response = as_actor(&node, &actor, Some(signer_key))
            .execute(CREATE)
            .await;
        assert!(
            refusal(&response).contains(MISMATCH),
            "{:?}",
            response.errors
        );
        assert_eq!(fact_count(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn execute_refuses_a_key_with_dac_bypass() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (actor, actor_key) = k1_key();
        let context = as_actor(&node, &actor, Some(actor_key)).with_dac_bypass(true);
        let response = context.execute(CREATE).await;
        assert!(refusal(&response).contains(BYPASS), "{:?}", response.errors);
        assert_eq!(fact_count(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn execute_refuses_an_unsupported_key() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (actor, mut actor_key) = k1_key();
        actor_key.key_type = SigningKeyType::Secp256r1;
        let response = as_actor(&node, &actor, Some(actor_key))
            .execute(CREATE)
            .await;
        assert!(
            refusal(&response).contains(UNSUPPORTED),
            "{:?}",
            response.errors
        );
        assert_eq!(fact_count(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn execute_signs_with_the_callers_own_key() {
    workers::block_on(async {
        let (node, _) = node().await;
        for (actor, actor_key) in [k1_key(), ed_key()] {
            let response = as_actor(&node, &actor, Some(actor_key))
                .execute(CREATE)
                .await;
            assert!(!response.has_errors(), "{:?}", response.errors);
            assert_eq!(composite_signer(&node).await, actor);
            node.with_identity(identity::Did::new(actor.as_str()).unwrap())
                .truncate_collection("Fact", None)
                .await
                .unwrap();
        }
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A key the process holds for the request's DID is not the caller's: only
/// the node's DID is looked up, so the node signs.
#[test]
fn execute_does_not_sign_with_a_key_registered_for_the_request_did() {
    workers::block_on(async {
        let (node, node_did) = node().await;
        let (actor, actor_key) = k1_key();
        defra_core::signing::store_identity(&actor, actor_key).unwrap();
        let response = as_actor(&node, &actor, None).execute(CREATE).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(composite_signer(&node).await, node_did);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn import_backup_refuses_a_key_of_another_actor() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (_signer, signer_key) = k1_key();
        let (actor, _) = k1_key();
        let error = as_actor(&node, &actor, Some(signer_key))
            .import_backup(BACKUP)
            .await
            .expect_err("the import must be refused");
        assert!(error.to_string().contains(MISMATCH), "{error}");
        assert_eq!(fact_count(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn import_backup_refuses_a_key_with_dac_bypass() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (actor, actor_key) = k1_key();
        let context = as_actor(&node, &actor, Some(actor_key)).with_dac_bypass(true);
        let error = context
            .import_backup(BACKUP)
            .await
            .expect_err("the import must be refused");
        assert!(error.to_string().contains(BYPASS), "{error}");
        assert_eq!(fact_count(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn import_backup_signs_with_the_callers_own_key() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (actor, actor_key) = k1_key();
        as_actor(&node, &actor, Some(actor_key))
            .import_backup(BACKUP)
            .await
            .unwrap();
        assert_eq!(composite_signer(&node).await, actor);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// Every other entry that takes a caller's key and writes refuses another
/// actor's key before it writes anything. Scoped node operations take no key.
#[test]
fn every_writing_entry_refuses_a_key_of_another_actor() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (signer, signer_key) = k1_key();
        let owner = as_actor(&node, &signer, Some(signer_key.clone()));
        let created = owner.execute(CREATE).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let (actor, _) = k1_key();
        let context = as_actor(&node, &actor, Some(signer_key));
        let response = context
            .execute_request(query::QueryRequest::new(CREATE))
            .await;
        let outcomes = [("execute_request", format!("{:?}", response.errors))];
        let admitted: Vec<_> = outcomes
            .iter()
            .filter(|(_, outcome)| !outcome.contains(MISMATCH))
            .collect();
        assert!(admitted.is_empty(), "not refused: {admitted:#?}");
        assert_eq!(fact_count(&node).await, 1);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}
