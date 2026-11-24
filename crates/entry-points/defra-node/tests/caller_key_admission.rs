//! Every `Node` entry that takes a caller's context admits the caller's key
//! before anything is written: the key must be a usable local Ed25519 or
//! secp256k1 key of the actor the context names, with DAC left on. A refusal
//! writes nothing, and a request that names an identity but brings no key
//! signs with the node's key, never with a key the process holds for that
//! identity.

use crypto::Key;
use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_core::RequestContext;
use defra_node::Node;
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

fn as_actor(actor: &str, key: Option<SigningConfig>) -> RequestContext {
    RequestContext::default()
        .with_identity(Some(actor.to_owned()))
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
fn execute_as_refuses_a_key_of_another_actor() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (_signer, signer_key) = k1_key();
        let (actor, _) = k1_key();
        let response = node
            .execute_as(&as_actor(&actor, Some(signer_key)), CREATE)
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
fn execute_as_refuses_a_key_with_dac_bypass() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (actor, actor_key) = k1_key();
        let context = as_actor(&actor, Some(actor_key)).with_dac_bypass(true);
        let response = node.execute_as(&context, CREATE).await;
        assert!(refusal(&response).contains(BYPASS), "{:?}", response.errors);
        assert_eq!(fact_count(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn execute_as_refuses_an_unsupported_key() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (actor, mut actor_key) = k1_key();
        actor_key.key_type = SigningKeyType::Secp256r1;
        let response = node
            .execute_as(&as_actor(&actor, Some(actor_key)), CREATE)
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
fn execute_as_signs_with_the_callers_own_key() {
    workers::block_on(async {
        let (node, _) = node().await;
        for (actor, actor_key) in [k1_key(), ed_key()] {
            let response = node
                .execute_as(&as_actor(&actor, Some(actor_key)), CREATE)
                .await;
            assert!(!response.has_errors(), "{:?}", response.errors);
            assert_eq!(composite_signer(&node).await, actor);
            let context = as_actor(&actor, None);
            node.truncate_collection_as(&context, "Fact", None)
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
fn execute_as_does_not_sign_with_a_key_registered_for_the_request_did() {
    workers::block_on(async {
        let (node, node_did) = node().await;
        let (actor, actor_key) = k1_key();
        defra_core::signing::store_identity(&actor, actor_key).unwrap();
        let response = node.execute_as(&as_actor(&actor, None), CREATE).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(composite_signer(&node).await, node_did);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn import_backup_as_refuses_a_key_of_another_actor() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (_signer, signer_key) = k1_key();
        let (actor, _) = k1_key();
        let error = node
            .import_backup_as(&as_actor(&actor, Some(signer_key)), BACKUP)
            .await
            .expect_err("the import must be refused");
        assert!(error.to_string().contains(MISMATCH), "{error}");
        assert_eq!(fact_count(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn import_backup_as_refuses_a_key_with_dac_bypass() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (actor, actor_key) = k1_key();
        let context = as_actor(&actor, Some(actor_key)).with_dac_bypass(true);
        let error = node
            .import_backup_as(&context, BACKUP)
            .await
            .expect_err("the import must be refused");
        assert!(error.to_string().contains(BYPASS), "{error}");
        assert_eq!(fact_count(&node).await, 0);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn import_backup_as_signs_with_the_callers_own_key() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (actor, actor_key) = k1_key();
        node.import_backup_as(&as_actor(&actor, Some(actor_key)), BACKUP)
            .await
            .unwrap();
        assert_eq!(composite_signer(&node).await, actor);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// Every other `_as` entry that writes refuses another actor's key before
/// it writes anything.
#[test]
fn every_writing_entry_refuses_a_key_of_another_actor() {
    workers::block_on(async {
        let (node, _) = node().await;
        let (signer, signer_key) = k1_key();
        let owner = as_actor(&signer, Some(signer_key.clone()));
        let created = node.execute_as(&owner, CREATE).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let (actor, _) = k1_key();
        let context = as_actor(&actor, Some(signer_key));
        let text = |result: Result<(), defra_node::Error>| match result {
            Ok(()) => "admitted".to_owned(),
            Err(error) => error.to_string(),
        };
        let response = node
            .execute_request_as(&context, query::QueryRequest::new(CREATE))
            .await
            .unwrap();
        let outcomes = [
            (
                "add_collection_as",
                text(
                    node.add_collection_as(&context, "type Other { value: String }")
                        .await
                        .map(drop),
                ),
            ),
            (
                "truncate_collection_as",
                text(node.truncate_collection_as(&context, "Fact", None).await),
            ),
            (
                "add_view_as",
                text(
                    node.add_view_as(
                        &context,
                        "Fact { value }",
                        "type FactView { value: String }",
                        None,
                    )
                    .await
                    .map(drop),
                ),
            ),
            (
                "refresh_views_as",
                text(node.refresh_views_as(&context, Default::default()).await),
            ),
            (
                "materialize_collection_as",
                text(
                    node.materialize_collection_as(&context, "Fact")
                        .await
                        .map(drop),
                ),
            ),
            (
                "patch_collection_as",
                text(node.patch_collection_as(&context, "[]", None).await),
            ),
            ("execute_request_as", format!("{:?}", response.errors)),
        ];
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
