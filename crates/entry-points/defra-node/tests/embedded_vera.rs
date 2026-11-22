//! A node that runs its own Vera node for document ACP.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use acp::{DocumentACP as _, DocumentPermission, Identity};
use defra_node::{
    EmbeddedVeraActor, EmbeddedVeraConfig, EmbeddedVeraDeployment, EmbeddedVeraPolicy, Node,
    PolicyNonce, QueryRequest, RelationshipChange,
};

#[cfg(feature = "http")]
#[path = "embedded_vera/verified.rs"]
mod verified;

#[cfg(feature = "http")]
#[path = "embedded_vera/clock_skew.rs"]
mod clock_skew;

#[cfg(feature = "http")]
#[path = "embedded_vera/http_staged.rs"]
mod http_staged;

#[path = "embedded_vera/first_start.rs"]
mod first_start;

#[path = "embedded_vera/projection.rs"]
mod projection;

#[path = "embedded_vera/registration_batch.rs"]
mod registration_batch;

#[path = "embedded_vera/replica.rs"]
mod replica;

#[path = "embedded_vera/settle_before_observing.rs"]
mod settle_before_observing;

#[path = "embedded_vera/rpc_port.rs"]
mod rpc_port;

#[path = "embedded_vera/strict_reads.rs"]
mod strict_reads;

#[cfg(feature = "http")]
#[path = "embedded_vera/verified_edit.rs"]
mod verified_edit;

#[cfg(feature = "http")]
#[path = "embedded_vera/owner_in_txn.rs"]
mod owner_in_txn;

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
    starting(directory, actor, policies, None)
        .await
        .expect("node")
}

/// A node whose Vera node also serves its JSON-RPC methods, and the
/// loopback address it serves them on.
async fn node_serving(
    directory: &Path,
    actor: EmbeddedVeraActor,
    policies: Vec<EmbeddedVeraPolicy>,
) -> (Node, std::net::SocketAddr) {
    rpc_port::on_free_rpc_port(|rpc| {
        starting(directory, actor.clone(), policies.clone(), Some(rpc))
    })
    .await
}

async fn starting(
    directory: &Path,
    actor: EmbeddedVeraActor,
    policies: Vec<EmbeddedVeraPolicy>,
    rpc_listen: Option<std::net::SocketAddr>,
) -> defra_node::Result<Node> {
    starting_with(directory, actor, policies, rpc_listen, |builder| builder).await
}

/// [`starting`], with the builder changed by `customize` before it builds.
async fn starting_with(
    directory: &Path,
    actor: EmbeddedVeraActor,
    policies: Vec<EmbeddedVeraPolicy>,
    rpc_listen: Option<std::net::SocketAddr>,
    customize: impl FnOnce(defra_node::NodeBuilder) -> defra_node::NodeBuilder,
) -> defra_node::Result<Node> {
    let keyring: Arc<dyn keyring::Keyring> = Arc::new(
        keyring::FileKeyring::open(directory.join("keys"), b"test-password").expect("keyring"),
    );
    customize(Node::builder())
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

async fn reads_doc(node: &Node, actor: &identity::Did, policy: &str, doc: &str) -> bool {
    node.document_acp()
        .check_doc_access(
            &Identity::Authenticated(actor.clone()),
            DocumentPermission::Read,
            policy,
            "users",
            doc,
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
            .add_policy(owner.as_str(), POLICY, None)
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
                .add_policy(owner.as_str(), POLICY, None)
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

/// [`POLICY`] seeded under `creator`, with the zero nonce.
fn seed(creator: &str) -> EmbeddedVeraPolicy {
    EmbeddedVeraPolicy {
        definition: POLICY.to_string(),
        nonce: PolicyNonce::default(),
        creator: creator.to_string(),
    }
}

/// The policy `creator`'s deployment was seeded with, found by `node` under
/// the ID computed beforehand and kept in its own store.
async fn seeded_policy(node: &Node, creator: &str) -> String {
    let id = seed(creator).id().expect("policy ID");
    let found = node
        .get_dac_policy_as(&defra_core::RequestContext::default(), &id)
        .await
        .expect("policy read")
        .expect("seeded policy");
    assert_eq!(found.id, id);
    id
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

/// A seed that does not compile is refused before the deployment exists, so
/// the host starts in the same directory once the seed is corrected.
#[test]
fn a_refused_seed_leaves_the_directory_free_for_a_corrected_one() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let actor = || EmbeddedVeraActor::Secp256k1Key(vec![42; 32]);
        let seed = |definition: &str| {
            vec![EmbeddedVeraPolicy {
                definition: definition.to_string(),
                nonce: PolicyNonce::default(),
                creator: STRANGER.to_string(),
            }]
        };

        let refused = starting(directory.path(), actor(), seed("["), None).await;
        assert!(refused.is_err(), "a malformed seed started a node");

        let node = node_with(directory.path(), actor(), seed(POLICY)).await;
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A host computes the ID of a policy it seeds its Vera node with before the
/// deployment exists; the node's own create of the definition under the
/// seed's nonce returns that policy and makes no second one, across a
/// restart, and another nonce makes a policy of its own.
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
        let seeded = seed(owner.as_str());
        let predicted = seeded.id().expect("policy ID");

        let first = node_with(
            directory.path(),
            EmbeddedVeraActor::Signer(Arc::clone(&signer)),
            vec![seeded.clone()],
        )
        .await;
        let adopted = first
            .add_dac_policy_with_nonce_as(&context, POLICY, seeded.nonce)
            .await
            .expect("seeded policy");
        assert_eq!(adopted, predicted);
        let another = first
            .add_dac_policy_with_nonce_as(&context, POLICY, PolicyNonce([1; 32]))
            .await
            .expect("a policy of its own");
        assert_ne!(another, adopted);
        assert!(first
            .add_dac_policy_with_counter_as(&context, POLICY, 0)
            .await
            .is_err());
        // The node creates as its actor, so another caller's nonce would name
        // a policy derived from the actor's DID, not the caller's own.
        let as_stranger =
            defra_core::RequestContext::default().with_identity(Some(STRANGER.into()));
        let refused = first
            .add_dac_policy_with_nonce_as(&as_stranger, POLICY, seeded.nonce)
            .await
            .expect_err("a nonce from a caller that is not the node's actor");
        assert!(refused.to_string().contains("node's actor"), "{refused}");
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
            vec![seeded.clone()],
        )
        .await;
        assert_eq!(
            second
                .add_dac_policy_with_nonce_as(&context, POLICY, seeded.nonce)
                .await
                .expect("seeded policy"),
            predicted
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

        let (first, rpc) = node_serving(
            &directory.path().join("first"),
            EmbeddedVeraActor::Signer(first_signer),
            vec![seed(owner.as_str())],
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
                genesis_id: None,
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
            .add_dac_policy_with_nonce_as(
                &defra_core::RequestContext::default().with_identity(Some(owner.to_string())),
                POLICY,
                PolicyNonce::default(),
            )
            .await
            .expect("seeded policy");
        assert_eq!(seeded_policy(&second, owner.as_str()).await, policy);

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

/// A caller whose key never reaches the node authenticates with a
/// delegation it signed for this node's worker, and the node's Vera ACP
/// acts under it: the caller owns what it creates.
#[cfg(feature = "http")]
#[test]
fn a_caller_that_keeps_its_key_writes_through_its_own_delegation() {
    use acp::RemotePolicies as _;
    use vera::vera_rs::{caller_delegation, DelegationSigner, Ed25519Signer};

    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(directory.path().join("keys"), b"test-password")
                .expect("keyring"),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = Node::builder()
            .with_http(defra_node::HttpConfig::with_listener(listener).unwrap().with_authenticated_routes(
                axum::Router::new().route("/application", axum::routing::get(|identity: defra_http::ExtractIdentity| async move {
                    axum::Json(serde_json::json!({"caller": identity.did().map(|did| did.as_str()), "expiration": identity.context().authorization_expiration()}))
                })),
            ))
            .with_embedded_vera(EmbeddedVeraConfig {
                data_dir: directory.path().join("vera"),
                deployment: EmbeddedVeraDeployment {
                    chain_id: 9404,
                    threshold: 1,
                    operator_keys: vec![ACCOUNT_KEY.to_string()],
                    created: now(),
                    policies: Vec::new(),
                },
                listen: "127.0.0.1:0".parse().expect("address"),
                start_deadline: Duration::from_secs(60),
                stop_deadline: Duration::from_secs(30),
                actor: EmbeddedVeraActor::Secp256k1Key(vec![42; 32]),
                rpc_listen: None,
                worker_dir: directory.path().join("worker"),
                keyring,
                tuning: vera::AcpTuning::default(),
            })
            .build()
            .await
            .expect("node");
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let owner = vera.self_did().expect("actor DID");
        let policy = vera.add_policy(&owner, POLICY, None).await.expect("policy");
        node.add_collection(&format!(
            "type Users @policy(id: \"{policy}\", resource: \"users\") {{ name: String }}"
        ))
        .await
        .expect("collection");

        let caller_key =
            identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let caller =
            Ed25519Signer::from_slice(&caller_key.private_key_bytes())
                .expect("key");
        let audience = vera
            .light_client_status()
            .expect("status")
            .delegation
            .expect("a vera.rs node names its delegation audience");
        let lifetime = Duration::from_secs(60);
        let delegation = caller_delegation(&caller, &audience, lifetime)
            .await
            .expect("delegation");
        let elsewhere = caller_delegation(
            &caller,
            &acp::DelegationAudience {
                worker: caller.did().to_owned(),
                ..audience.clone()
            },
            lifetime,
        )
        .await
        .expect("delegation");

        let client = net::http::Client::builder(edge::testing::edge())
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let create = |token: String| {
            client
                .post(format!("http://{address}/api/v0/graphql"))
                .header("Authorization", format!("Bearer {token}"))
                .json(&serde_json::json!({
                    "query": "mutation { add_Users(input: {name: \"kept\"}) { _docID } }"
                }))
                .send()
        };

        let application = client.get(format!("http://{address}/application"))
            .header("Authorization", format!("Bearer {delegation}")).send().await.unwrap();
        assert!(application.status().is_success());
        let authenticated: serde_json::Value = application.json().await.unwrap();
        let verified = node.document_acp().caller_of_delegation(&delegation).unwrap().unwrap();
        assert_eq!(authenticated["caller"], caller.did());
        assert_eq!(authenticated["expiration"], verified.expires_at);
        let wrong_application = client.get(format!("http://{address}/application"))
            .header("Authorization", format!("Bearer {elsewhere}")).send().await.unwrap();
        assert_eq!(wrong_application.status().as_u16(), 403);

        let refused = create(elsewhere.clone()).await.unwrap();
        assert_eq!(refused.status().as_u16(), 403);

        let created = create(delegation.clone()).await.unwrap();
        assert!(created.status().is_success(), "{}", created.status());
        let body: serde_json::Value = created.json().await.unwrap();
        let doc = body["data"]["add_Users"][0]["_docID"]
            .as_str()
            .unwrap_or_else(|| panic!("no document: {body}"))
            .to_owned();
        let registered = node
            .document_acp()
            .get_doc_owner(&policy, "users", &doc)
            .await
            .expect("owner");
        assert_eq!(
            registered.as_ref().map(|did| did.as_str()),
            Some(caller.did())
        );

        let signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: caller_key.private_key_bytes(),
            public_key_bytes: caller_key.public_key_bytes(),
            public_key_hex: hex::encode(caller_key.public_key_bytes()),
            remote_signer: None,
            signing_authorization: None,
        };
        assert!(defra_core::signing::get_identity(caller.did()).is_none());
        let schema = node.get_collection("Users").unwrap().unwrap();
        let input = serde_json::json!({"name": "signed by held key"});
        let mut graph = db::block::builder::ClientCreateSignatures::from_input(
            &storage::Crdts::default(),
            &schema,
            &input,
            &signing,
        )
        .unwrap();
        let query = query::QueryRequest::new(
            "mutation($input: UsersMutationInputArg!) { add_Users(input: $input) { _docID } }",
        )
        .with_variables(serde_json::json!({"input": input}));
        let endpoint = format!("ws://{address}/api/v0/graphql/client-sign");
        assert!(defra_http::client_signed_create::execute(
            &endpoint,
            &format!("Bearer {elsewhere}"),
            query.clone(),
            None,
            &mut graph,
        )
        .await
        .is_err());
        let signed = defra_http::client_signed_create::execute(
            &endpoint,
            &format!("Bearer {delegation}"),
            query,
            None,
            &mut graph,
        )
        .await
        .expect("the delegation authenticates the signing socket");
        assert!(!signed.has_errors(), "{:?}", signed.errors);
        assert_eq!(
            node.document_acp()
                .get_doc_owner(&policy, "users", graph.doc_id())
                .await
                .unwrap()
                .as_ref()
                .map(|did| did.as_str()),
            Some(caller.did())
        );
        assert!(defra_core::signing::get_identity(caller.did()).is_none());

        // The caller grants another actor access the same way, and that actor
        // then reads with an ordinary identity token.
        let reader =
            identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let reader_did = {
            use identity::Identity as _;
            reader.did().unwrap()
        };
        let granting = caller_delegation(&caller, &audience, lifetime)
            .await
            .expect("delegation");
        let granted = client
            .post(format!("http://{address}/api/v0/acp/document/relationship"))
            .header("Authorization", format!("Bearer {granting}"))
            .json(&serde_json::json!({
                "collection": "Users",
                "doc_id": doc,
                "relation": "reader",
                "target_actor": reader_did.as_str(),
            }))
            .send()
            .await
            .unwrap();
        assert!(granted.status().is_success(), "{}", granted.status());
        assert!(reads_doc(&node, &reader_did, &policy, &doc).await);
        assert!(!reads_doc(&node, &identity::Did::new(STRANGER).unwrap(), &policy, &doc).await);

        let token = identity::new_token(
            &reader,
            Duration::from_secs(60),
            Some(address.to_string()),
            None,
        )
        .unwrap();
        let read = client
            .post(format!("http://{address}/api/v0/graphql"))
            .header(
                "Authorization",
                format!("Bearer {}", String::from_utf8(token).unwrap()),
            )
            .json(&serde_json::json!({"query": "{ Users { name } }"}))
            .send()
            .await
            .unwrap();
        assert!(read.status().is_success(), "{}", read.status());
        let body: serde_json::Value = read.json().await.unwrap();
        assert_eq!(body["data"]["Users"][0]["name"], "kept", "{body}");

        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// Whether the Vera node itself lets `actor` read `doc`, whatever this node
/// still owes it.
async fn chain_reads(node: &Node, actor: &identity::Did, policy: &str, doc: &str) -> bool {
    node.vera_acp()
        .expect("vera.rs document ACP")
        .check_doc_access(
            &Identity::Authenticated(actor.clone()),
            DocumentPermission::Read,
            policy,
            "users",
            doc,
        )
        .await
        .expect("access decision")
}

fn users(policy: &str) -> String {
    format!("type Users @policy(id: \"{policy}\", resource: \"users\") {{ name: String }}")
}

fn create_user(name: &str, creator: &identity::Did) -> QueryRequest {
    QueryRequest::new(format!(
        "mutation {{ add_Users(input: {{name: \"{name}\"}}) {{ _docID }} }}"
    ))
    .with_identity(Some(creator.clone()))
}

fn created(response: defra_node::QueryResponse) -> String {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("data")["add_Users"][0]["_docID"]
        .as_str()
        .expect("document ID")
        .to_owned()
}

fn read_users(reader: &identity::Did) -> QueryRequest {
    QueryRequest::new("{ Users { name } }").with_identity(Some(reader.clone()))
}

fn names(response: defra_node::QueryResponse) -> Vec<String> {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("data")["Users"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|user| user["name"].as_str().expect("name").to_owned())
        .collect()
}

/// A relationship granted on a document created in the same transaction is
/// owed to the Vera node with the registration, grants nothing until the
/// transaction commits, and lets the grantee read once it has.
#[test]
fn a_grant_staged_beside_its_document_lets_the_grantee_read_after_the_commit() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let reader = identity::Did::new(STRANGER).unwrap();
        let policy = vera
            .add_policy(owner.as_str(), POLICY, None)
            .await
            .expect("policy");
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let acting = defra_core::RequestContext::default().with_identity(Some(owner.to_string()));

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let doc = created(
            transaction
                .execute_request(create_user("fresh", &owner))
                .await,
        );
        let staged = node
            .add_dac_actor_relationship_change_for_in_txn(
                transaction.handle(),
                &acting,
                "Users",
                &doc,
                "reader",
                reader.as_str(),
            )
            .await
            .expect("staged grant");
        assert!(
            matches!(
                staged,
                RelationshipChange::Staged {
                    authority_checked: true,
                    ..
                }
            ),
            "{staged:?}"
        );
        assert!(names(transaction.execute_request(read_users(&reader)).await).is_empty());
        assert!(!chain_reads(&node, &reader, &policy, &doc).await);
        transaction.commit().await.expect("commit");

        assert!(chain_reads(&node, &reader, &policy, &doc).await);
        assert!(reads_doc(&node, &reader, &policy, &doc).await);
        assert_eq!(
            names(node.execute_request(read_users(&reader)).await),
            ["fresh"]
        );
        // The advisory check at staging asks for a managing relation this way.
        assert!(vera
            .check_named_permission(&reader, "reader", &policy, "users", &doc)
            .await
            .expect("relation check"));
        assert!(!vera
            .check_named_permission(&reader, "writer", &policy, "users", &doc)
            .await
            .expect("relation check"));
        vera.add_actor_relationship(
            &owner,
            &identity::Did::wildcard(),
            &policy,
            "users",
            &doc,
            "reader",
            &[],
        )
        .await
        .expect("wildcard grant");
        assert!(vera
            .observe_actor_relationship(acp::ActorRelationshipObservation {
                requester: &owner,
                target: &reader,
                policy: &policy,
                resource: "users",
                document: &doc,
                relation: "reader",
            })
            .await
            .expect("direct grant"));
        assert!(!vera
            .observe_actor_relationship(acp::ActorRelationshipObservation {
                requester: &owner,
                target: &owner,
                policy: &policy,
                resource: "users",
                document: &doc,
                relation: "reader",
            })
            .await
            .expect("exact absence"));
        // A caller who may not manage the relation is refused alike for a
        // registered document and for one nobody registered.
        for document in [doc.as_str(), "bae-never-registered"] {
            let refused = vera
                .observe_actor_relationship(acp::ActorRelationshipObservation {
                    requester: &reader,
                    target: &reader,
                    policy: &policy,
                    resource: "users",
                    document,
                    relation: "reader",
                })
                .await;
            assert!(
                matches!(&refused, Err(acp::Error::NotManager { operation }) if operation == "observe relationship"),
                "{document}: {refused:?}"
            );
        }
        drop(vera);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// An exact observation repeated while every write finalizes a block: a block
/// between the observation's reads is an overtaken observation, asked again,
/// never a failure (`proofs/tla/ExactRelationshipObservation.tla`).
#[test]
fn an_observation_under_writes_is_asked_again_when_a_block_overtakes_it() {
    const WRITES: usize = 60;
    const OBSERVATIONS: usize = 120;
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let reader = identity::Did::new(STRANGER).unwrap();
        let policy = vera
            .add_policy(owner.as_str(), POLICY, None)
            .await
            .expect("policy");
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let doc = created(node.execute_request(create_user("observed", &owner)).await);
        vera.add_actor_relationship(&owner, &reader, &policy, "users", &doc, "reader", &[])
            .await
            .expect("grant");

        let writes = |lane: usize| {
            let (node, owner) = (&node, &owner);
            async move {
                for index in 0..WRITES {
                    created(
                        node.execute_request(create_user(&format!("write {lane}-{index}"), owner))
                            .await,
                    );
                }
            }
        };
        let observations = async {
            for _ in 0..OBSERVATIONS {
                let observed = vera
                    .observe_actor_relationship(acp::ActorRelationshipObservation {
                        requester: &owner,
                        target: &reader,
                        policy: &policy,
                        resource: "users",
                        document: &doc,
                        relation: "reader",
                    })
                    .await;
                assert!(matches!(observed, Ok(true)), "{observed:?}");
            }
        };
        futures::join!(writes(0), writes(1), observations);
        drop(vera);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// The grants one transaction stages go to the Vera node in one request,
/// which it applies whole: both grantees read once the commit returns.
#[test]
fn the_grants_one_transaction_stages_land_together() {
    use vera::vera_rs::{DelegationSigner, Ed25519Signer};

    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let first = identity::Did::new(STRANGER).unwrap();
        let second =
            identity::Did::new(Ed25519Signer::from_slice(&[44; 32]).expect("key").did()).unwrap();
        let policy = vera
            .add_policy(owner.as_str(), POLICY, None)
            .await
            .expect("policy");
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let acting = defra_core::RequestContext::default().with_identity(Some(owner.to_string()));
        let doc = created(node.execute_request(create_user("shared", &owner)).await);

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        for grantee in [&first, &second] {
            let staged = node
                .add_dac_actor_relationship_change_for_in_txn(
                    transaction.handle(),
                    &acting,
                    "Users",
                    &doc,
                    "reader",
                    grantee.as_str(),
                )
                .await
                .expect("staged grant");
            assert!(
                matches!(staged, RelationshipChange::Staged { .. }),
                "{staged:?}"
            );
        }
        transaction.commit().await.expect("commit");

        assert!(chain_reads(&node, &first, &policy, &doc).await);
        assert!(chain_reads(&node, &second, &policy, &doc).await);
        assert!(node
            .owed_acp_as(&acting, owner.as_str())
            .await
            .expect("what is owed")
            .is_empty());
        drop(vera);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A revocation staged in a transaction denies its subject through that
/// transaction at once and on this node from the commit; the Vera node holds
/// it once it is settled, which the commit waits for when the chain answers.
#[test]
fn a_revocation_staged_in_a_transaction_denies_at_the_commit_and_the_chain_follows() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let reader = identity::Did::new(STRANGER).unwrap();
        let policy = vera
            .add_policy(owner.as_str(), POLICY, None)
            .await
            .expect("policy");
        drop(vera);
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let acting = defra_core::RequestContext::default().with_identity(Some(owner.to_string()));
        let doc = created(node.execute_request(create_user("shared", &owner)).await);
        node.add_dac_actor_relationship(owner.as_str(), "Users", &doc, "reader", reader.as_str())
            .await
            .expect("grant");
        assert!(chain_reads(&node, &reader, &policy, &doc).await);

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        assert_eq!(
            names(transaction.execute_request(read_users(&reader)).await),
            ["shared"]
        );
        let staged = node
            .delete_dac_actor_relationship_change_for_in_txn(
                transaction.handle(),
                &acting,
                "Users",
                &doc,
                "reader",
                reader.as_str(),
            )
            .await
            .expect("staged revocation");
        assert!(
            matches!(staged, RelationshipChange::Staged { .. }),
            "{staged:?}"
        );
        assert!(names(transaction.execute_request(read_users(&reader)).await).is_empty());
        assert_eq!(
            names(transaction.execute_request(read_users(&owner)).await),
            ["shared"]
        );
        // Nothing is committed: the grant still stands everywhere else.
        assert!(reads_doc(&node, &reader, &policy, &doc).await);
        assert!(chain_reads(&node, &reader, &policy, &doc).await);
        transaction.commit().await.expect("commit");

        assert!(!reads_doc(&node, &reader, &policy, &doc).await);
        assert!(names(node.execute_request(read_users(&reader)).await).is_empty());
        assert!(!chain_reads(&node, &reader, &policy, &doc).await);
        assert!(reads_doc(&node, &owner, &policy, &doc).await);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A document and a grant on it committed while the Vera deployment cannot
/// be reached stay owed across a restart, and the node that comes back
/// registers the document and then submits the grant.
#[test]
fn a_restart_settles_a_change_the_chain_could_not_take() {
    use defra_node::{VeraRsActor, VeraRsConfig};
    use vera::vera_rs::{DelegationSigner, Ed25519Signer};

    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let chain_signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[42; 32]).expect("key"));
        let signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[43; 32]).expect("key"));
        let chain_owner = identity::Did::new(chain_signer.did()).unwrap();
        let creator = identity::Did::new(signer.did()).unwrap();
        let reader = identity::Did::new(STRANGER).unwrap();
        let acting = defra_core::RequestContext::default().with_identity(Some(creator.to_string()));

        // The deployment runs in one node; the node under test verifies
        // against it and keeps its own store on disk.
        let chain_dir = directory.path().join("chain");
        let chain = || {
            node_serving(
                &chain_dir,
                EmbeddedVeraActor::Signer(Arc::clone(&chain_signer)),
                vec![seed(chain_owner.as_str())],
            )
        };
        let consumer = |trust: defra_node::EmbeddedVeraTrust, rpc: std::net::SocketAddr| {
            let home = directory.path().join("consumer");
            let signer = Arc::clone(&signer);
            async move {
                let keyring: Arc<dyn keyring::Keyring> = Arc::new(
                    keyring::FileKeyring::open(home.join("keys"), b"test-password")
                        .expect("keyring"),
                );
                Node::builder()
                    .data_path(home.join("data"))
                    .with_vera_rs(VeraRsConfig {
                        rpc_address: format!("http://{rpc}"),
                        consensus_key: trust.consensus_key,
                        genesis_id: None,
                        deployment_id: trust.chain_id,
                        actor: VeraRsActor::Signer(signer),
                        worker_dir: home.join("worker"),
                        keyring,
                        tuning: vera::AcpTuning::default(),
                    })
                    .build()
                    .await
                    .expect("consumer node")
            }
        };

        let (first_chain, rpc) = chain().await;
        let trust = first_chain
            .embedded_vera_trust()
            .expect("deployment")
            .clone();
        let node = consumer(trust, rpc).await;
        let policy = seeded_policy(&node, chain_owner.as_str()).await;
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        first_chain.shutdown().await.expect("shutdown");
        drop(first_chain);

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let doc = created(
            transaction
                .execute_request(create_user("kept", &creator))
                .await,
        );
        let staged = node
            .add_dac_actor_relationship_change_for_in_txn(
                transaction.handle(),
                &acting,
                "Users",
                &doc,
                "reader",
                reader.as_str(),
            )
            .await
            .expect("staged grant");
        assert!(
            matches!(staged, RelationshipChange::Staged { .. }),
            "{staged:?}"
        );
        transaction.commit().await.expect("commit");
        node.shutdown().await.expect("shutdown");
        drop(node);

        let (second_chain, rpc) = chain().await;
        let trust = second_chain
            .embedded_vera_trust()
            .expect("deployment")
            .clone();
        assert!(!reads_doc(&second_chain, &reader, &policy, &doc).await);
        let node = consumer(trust, rpc).await;
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !reads_doc(&second_chain, &reader, &policy, &doc).await {
            assert!(
                std::time::Instant::now() < deadline,
                "the owed grant was not settled at start"
            );
            edge::testing::edge()
                .sleep(Duration::from_millis(200))
                .await;
        }
        let owner = second_chain
            .document_acp()
            .get_doc_owner(&policy, "users", &doc)
            .await
            .expect("owner");
        assert_eq!(owner.as_ref(), Some(&creator));
        assert_eq!(
            names(node.execute_request(read_users(&reader)).await),
            ["kept"]
        );
        node.shutdown().await.expect("shutdown");
        second_chain.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A grant committed while the Vera deployment cannot be reached is
/// settled by the node's drainer once the deployment is back, with no
/// further request to the node and no restart of it: the drainer backs off
/// while the provider is away and tries again by itself.
#[test]
fn an_idle_node_settles_a_staged_grant_once_its_deployment_is_back() {
    use defra_node::{VeraRsActor, VeraRsConfig};
    use vera::vera_rs::{DelegationSigner, Ed25519Signer};

    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let chain_signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[42; 32]).expect("key"));
        let signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&[43; 32]).expect("key"));
        let chain_owner = identity::Did::new(chain_signer.did()).unwrap();
        let creator = identity::Did::new(signer.did()).unwrap();
        let reader = identity::Did::new(STRANGER).unwrap();
        let acting = defra_core::RequestContext::default().with_identity(Some(creator.to_string()));
        let chain_dir = directory.path().join("chain");
        let policies = vec![seed(chain_owner.as_str())];

        let (first_chain, rpc) = node_serving(
            &chain_dir,
            EmbeddedVeraActor::Signer(Arc::clone(&chain_signer)),
            policies.clone(),
        )
        .await;
        let trust = first_chain
            .embedded_vera_trust()
            .expect("deployment")
            .clone();
        let home = directory.path().join("consumer");
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(home.join("keys"), b"test-password").expect("keyring"),
        );
        let node = Node::builder()
            .data_path(home.join("data"))
            .with_vera_rs(VeraRsConfig {
                rpc_address: format!("http://{rpc}"),
                consensus_key: trust.consensus_key,
                genesis_id: None,
                deployment_id: trust.chain_id,
                actor: VeraRsActor::Signer(Arc::clone(&signer)),
                worker_dir: home.join("worker"),
                keyring,
                tuning: vera::AcpTuning::default(),
            })
            .build()
            .await
            .expect("consumer node");
        let policy = seeded_policy(&node, chain_owner.as_str()).await;
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        first_chain.shutdown().await.expect("shutdown");
        drop(first_chain);

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let doc = created(
            transaction
                .execute_request(create_user("kept", &creator))
                .await,
        );
        let staged = node
            .add_dac_actor_relationship_change_for_in_txn(
                transaction.handle(),
                &acting,
                "Users",
                &doc,
                "reader",
                reader.as_str(),
            )
            .await
            .expect("staged grant");
        assert!(
            matches!(staged, RelationshipChange::Staged { .. }),
            "{staged:?}"
        );
        transaction.commit().await.expect("commit");

        // The deployment comes back on the address the node knows. Nothing
        // is asked of the node from here on.
        let second_chain = starting(
            &chain_dir,
            EmbeddedVeraActor::Signer(Arc::clone(&chain_signer)),
            policies,
            Some(rpc),
        )
        .await
        .expect("the deployment back on its address");
        let deadline = std::time::Instant::now() + Duration::from_secs(150);
        while !reads_doc(&second_chain, &reader, &policy, &doc).await {
            assert!(
                std::time::Instant::now() < deadline,
                "the idle node did not settle its staged grant"
            );
            edge::testing::edge()
                .sleep(Duration::from_millis(500))
                .await;
        }
        let owner = second_chain
            .document_acp()
            .get_doc_owner(&policy, "users", &doc)
            .await
            .expect("owner");
        assert_eq!(owner.as_ref(), Some(&creator));
        node.shutdown().await.expect("shutdown");
        second_chain.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

const GROUPS_POLICY: &str = "\
name: embedded groups
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
          - groups->member
  - name: groups
    relations:
      - name: member
        types:
          - actor
";

/// The userset every member of group `g` stands for, as a relationship target.
const MEMBERS: &str = "groups:g#member";

/// An identity whose key the node holds, so the node signs for it, and which
/// is not the node's own actor.
fn registered_identity() -> identity::Did {
    use crypto::Key as _;
    use identity::Identity as _;

    let key = crypto::generate_ed25519().expect("key");
    let identity = identity::RawIdentity::from_ed25519(key.clone()).expect("identity");
    let did = identity.did().expect("did");
    let public = identity.public_key_bytes();
    defra_core::signing::store_identity(
        did.as_str(),
        defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: key.raw().to_vec(),
            public_key_hex: hex::encode(&public),
            public_key_bytes: public,
            remote_signer: None,
            signing_authorization: None,
        },
    )
    .expect("the store takes the identity");
    did
}

/// A node with a `Users` collection under [`GROUPS_POLICY`] and a group `g`
/// with `member` in it. `own` is a document of the node's own actor, the
/// case where a change sent under the node's token instead of its
/// requestor's would be accepted. `theirs` is a document of `owner`, who is
/// not the node's actor.
struct Grouped {
    node: Node,
    policy: String,
    actor: identity::Did,
    own: String,
    owner: identity::Did,
    theirs: String,
    member: identity::Did,
}

impl Grouped {
    async fn new(directory: &Path) -> Self {
        let node = node(directory).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let actor = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let policy = vera
            .add_policy(actor.as_str(), GROUPS_POLICY, None)
            .await
            .expect("policy");
        drop(vera);
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let owner = registered_identity();
        assert_ne!(owner, actor);
        let member = identity::Did::new(STRANGER).unwrap();
        let own = created(node.execute_request(create_user("own", &actor)).await);
        let theirs = created(node.execute_request(create_user("theirs", &owner)).await);
        node.document_acp()
            .register_object(&owner, &policy, "groups", "g")
            .await
            .expect("group");
        node.document_acp()
            .add_actor_relationship(&owner, &member, &policy, "groups", "g", "member", &[])
            .await
            .expect("membership");
        Self {
            node,
            policy,
            actor,
            own,
            owner,
            theirs,
            member,
        }
    }

    async fn member_reads(&self, doc: &str) -> bool {
        chain_reads(&self.node, &self.member, &self.policy, doc).await
    }
}

/// Whether `result` is the chain's rejection of a submitted relationship
/// change, and not a refusal made on this node.
fn rejected_by_the_chain(result: &defra_node::Result<bool>, verb: &str) -> bool {
    matches!(
        result,
        Err(error @ defra_node::Error::RelationshipAcp { verb: named, .. })
            if *named == verb && error.to_string().contains("was rejected")
    )
}

/// A userset relationship changed outside a transaction is decided by the
/// chain on its requestor's authority. On a document the node's own actor
/// owns, an outsider's grant and revocation are rejected: sent under the
/// node's token they would be accepted. The node's actor and another owner
/// each grant on their own document.
#[test]
fn a_direct_userset_change_is_decided_on_its_requestors_authority() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let grouped = Grouped::new(directory.path()).await;
        let node = &grouped.node;
        let outsider = registered_identity();
        let grant = |requestor: &identity::Did, doc: &str| {
            let (requestor, doc) = (requestor.to_string(), doc.to_owned());
            async move {
                node.add_dac_actor_relationship(&requestor, "Users", &doc, "reader", MEMBERS)
                    .await
            }
        };
        let revoke = |requestor: &identity::Did, doc: &str| {
            let (requestor, doc) = (requestor.to_string(), doc.to_owned());
            async move {
                node.delete_dac_actor_relationship(&requestor, "Users", &doc, "reader", MEMBERS)
                    .await
            }
        };
        assert!(!grouped.member_reads(&grouped.own).await);

        let refused = grant(&outsider, &grouped.own).await;
        assert!(rejected_by_the_chain(&refused, "add"), "{refused:?}");
        assert!(!grouped.member_reads(&grouped.own).await);

        grant(&grouped.actor, &grouped.own)
            .await
            .expect("the node actor's grant on its document");
        assert!(grouped.member_reads(&grouped.own).await);
        grant(&grouped.owner, &grouped.theirs)
            .await
            .expect("another owner's grant on its document");
        assert!(grouped.member_reads(&grouped.theirs).await);
        let mut read = names(node.execute_request(read_users(&grouped.member)).await);
        read.sort();
        assert_eq!(read, ["own", "theirs"]);

        let refused = revoke(&outsider, &grouped.own).await;
        assert!(rejected_by_the_chain(&refused, "delete"), "{refused:?}");
        // The node's actor has no authority on the other owner's document.
        let refused = revoke(&grouped.actor, &grouped.theirs).await;
        assert!(rejected_by_the_chain(&refused, "delete"), "{refused:?}");
        assert!(grouped.member_reads(&grouped.own).await);
        assert!(grouped.member_reads(&grouped.theirs).await);

        revoke(&grouped.actor, &grouped.own)
            .await
            .expect("the node actor's revocation");
        revoke(&grouped.owner, &grouped.theirs)
            .await
            .expect("the other owner's revocation");
        assert!(!grouped.member_reads(&grouped.own).await);
        assert!(!grouped.member_reads(&grouped.theirs).await);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// The same through a transaction. An outsider's userset grant on the node
/// actor's document is refused when it is staged. An owner that is not the
/// node's actor stages a grant and a revocation on its own document, and the
/// chain accepts them: submitted under the node's token it would reject
/// them.
#[test]
fn a_staged_userset_change_is_decided_on_its_actors_authority() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let grouped = Grouped::new(directory.path()).await;
        let node = &grouped.node;
        let outsider = registered_identity();
        let acting = |did: &identity::Did| {
            defra_core::RequestContext::default().with_identity(Some(did.to_string()))
        };

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let stage = |actor: &identity::Did, doc: &str| {
            let (context, doc, handle) = (acting(actor), doc.to_owned(), transaction.handle());
            async move {
                node.add_dac_actor_relationship_change_for_in_txn(
                    handle, &context, "Users", &doc, "reader", MEMBERS,
                )
                .await
            }
        };
        for doc in [&grouped.own, &grouped.theirs] {
            let refused = stage(&outsider, doc).await;
            assert!(
                matches!(
                    refused,
                    Err(defra_node::Error::Acp(acp::Error::NotOwner { .. }))
                ),
                "{refused:?}"
            );
        }
        for (actor, doc) in [
            (&grouped.actor, &grouped.own),
            (&grouped.owner, &grouped.theirs),
        ] {
            let staged = stage(actor, doc).await.expect("an owner's staged grant");
            assert!(
                matches!(
                    staged,
                    RelationshipChange::Staged {
                        authority_checked: true,
                        ..
                    }
                ),
                "{staged:?}"
            );
        }
        assert!(!grouped.member_reads(&grouped.theirs).await);
        transaction.commit().await.expect("commit");
        assert!(grouped.member_reads(&grouped.own).await);
        assert!(grouped.member_reads(&grouped.theirs).await);

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        node.delete_dac_actor_relationship_change_for_in_txn(
            transaction.handle(),
            &acting(&grouped.owner),
            "Users",
            &grouped.theirs,
            "reader",
            MEMBERS,
        )
        .await
        .expect("the owner's staged revocation");
        transaction.commit().await.expect("commit");
        assert!(!grouped.member_reads(&grouped.theirs).await);
        assert!(grouped.member_reads(&grouped.own).await);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A secp256k1 actor registered with the node under the compressed spelling
/// of its key, which is not the one DefraDB derives, and its DID as it was
/// registered.
fn registered_under_the_compressed_spelling() -> identity::Did {
    use crypto::Key as _;
    use identity::Identity as _;

    let key = crypto::generate_secp256k1().expect("key");
    let identity = identity::RawIdentity::from_secp256k1(key.clone()).expect("identity");
    let derived = identity.did().expect("did");
    let (kind, point) = crypto::parse_did_key(derived.as_str()).expect("did:key");
    assert_eq!(point.len(), 65);
    let mut compressed = vec![0x02 | (point[64] & 1)];
    compressed.extend_from_slice(&point[1..33]);
    let did = identity::Did::new(crypto::create_did_key(kind, &compressed).expect("did:key"))
        .expect("DID");
    assert_ne!(did, derived);
    let public = identity.public_key_bytes();
    defra_core::signing::store_identity(
        did.as_str(),
        defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Secp256k1,
            private_key_bytes: key.raw().to_vec(),
            public_key_hex: hex::encode(&public),
            public_key_bytes: public,
            remote_signer: None,
            signing_authorization: None,
        },
    )
    .expect("the store takes the identity");
    did
}

/// An owner whose key is registered under another spelling than the one a
/// staged change keeps stages a grant and a revocation on its document, and
/// the chain takes both: the key is found at submission as it was at staging.
#[test]
fn a_key_registered_under_another_spelling_signs_its_staged_changes() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let actor = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let policy = vera
            .add_policy(actor.as_str(), POLICY, None)
            .await
            .expect("policy");
        drop(vera);
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let owner = registered_under_the_compressed_spelling();
        let reader = identity::Did::new(STRANGER).unwrap();
        let acting = defra_core::RequestContext::default().with_identity(Some(owner.to_string()));
        let doc = created(node.execute_request(create_user("theirs", &owner)).await);

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let staged = node
            .add_dac_actor_relationship_change_for_in_txn(
                transaction.handle(),
                &acting,
                "Users",
                &doc,
                "reader",
                reader.as_str(),
            )
            .await
            .expect("staged grant");
        assert!(
            matches!(
                staged,
                RelationshipChange::Staged {
                    authority_checked: true,
                    ..
                }
            ),
            "{staged:?}"
        );
        transaction.commit().await.expect("commit");
        assert!(chain_reads(&node, &reader, &policy, &doc).await);

        let transaction = node.begin_transaction_guard(false).await.unwrap();
        node.delete_dac_actor_relationship_change_for_in_txn(
            transaction.handle(),
            &acting,
            "Users",
            &doc,
            "reader",
            reader.as_str(),
        )
        .await
        .expect("staged revocation");
        transaction.commit().await.expect("commit");
        assert!(!chain_reads(&node, &reader, &policy, &doc).await);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
