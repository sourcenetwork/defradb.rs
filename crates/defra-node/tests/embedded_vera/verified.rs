//! Verified writes against a real chain: the node acts for a signer whose
//! key it does not hold under the signer's delegation, plain or bound to the
//! one command the write sends, and the chain records the signer as owner.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use acp::{DocumentACP as _, RemotePolicies as _};
use crypto::{Key as _, PrivateKey as _};
use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_node::{
    EmbeddedVeraActor, EmbeddedVeraConfig, EmbeddedVeraDeployment, Node, QueryRequest,
    QueryResponse, RelationshipChange, SignerCredential,
};
use query::executor::WRITE_AUTHORITY_UNAVAILABLE_ERROR_CODE;
use serde_json::json;
use vera::vera_rs::{
    caller_delegation, request_delegation, request_digest, DelegationSigner, Ed25519Signer,
    RequestBinding,
};
use vera::{PolicyAction, PolicyCommand, PolicySubject, SubjectRef};

use super::{chain_reads, now, users, ACCOUNT_KEY, POLICY, STRANGER};

const LIFETIME: Duration = Duration::from_secs(120);

/// A node on its own Vera deployment, its HTTP API and the deployment's
/// JSON-RPC methods both served on loopback.
struct Deployed {
    node: Node,
    http: SocketAddr,
    rpc: SocketAddr,
    policy: String,
    audience: acp::DelegationAudience,
    _directory: tempfile::TempDir,
}

/// A user who signs its own blocks and delegations, and whose key the node
/// never sees.
struct User {
    did: identity::Did,
    blocks: SigningConfig,
    delegations: Ed25519Signer,
}

impl User {
    fn new() -> Self {
        let private = crypto::generate_ed25519().unwrap();
        let public = private.public_key();
        let delegations = Ed25519Signer::from_slice(private.raw()).expect("key");
        Self {
            did: identity::Did::new(delegations.did()).unwrap(),
            blocks: SigningConfig {
                key_type: SigningKeyType::Ed25519,
                private_key_bytes: private.raw_owned(),
                public_key_bytes: public.raw_owned(),
                public_key_hex: hex::encode(public.raw()),
                remote_signer: None,
                signing_authorization: None,
            },
            delegations,
        }
    }
}

/// One verified create handed to the node.
struct Attempt {
    doc: String,
    txn: defra_node::TransactionHandle,
    response: QueryResponse,
}

impl Deployed {
    async fn start(chain_id: u64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(directory.path().join("keys"), b"test-password")
                .expect("keyring"),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let http = listener.local_addr().unwrap();
        let rpc = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .unwrap();
        let node = Node::builder()
            .enable_signing()
            .with_http(defra_node::HttpConfig::with_listener(listener).unwrap())
            .with_embedded_vera(EmbeddedVeraConfig {
                data_dir: directory.path().join("vera"),
                deployment: EmbeddedVeraDeployment {
                    chain_id,
                    threshold: 1,
                    operator_keys: vec![ACCOUNT_KEY.to_string()],
                    created: now(),
                    policies: Vec::new(),
                },
                listen: "127.0.0.1:0".parse().expect("address"),
                start_deadline: Duration::from_secs(60),
                stop_deadline: Duration::from_secs(30),
                actor: EmbeddedVeraActor::Secp256k1Key(vec![42; 32]),
                rpc_listen: Some(rpc),
                worker_dir: directory.path().join("worker"),
                keyring,
                tuning: vera::AcpTuning::default(),
            })
            .build()
            .await
            .expect("node");
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let admin = vera.self_did().expect("actor DID");
        let policy = vera.add_policy(&admin, POLICY).await.expect("policy");
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let audience = vera
            .light_client_status()
            .expect("status")
            .delegation
            .expect("a vera.rs node names its delegation audience");
        Self {
            node,
            http,
            rpc,
            policy,
            audience,
            _directory: directory,
        }
    }

    fn client() -> net::http::Client {
        net::http::Client::builder(edge::testing::edge())
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap()
    }

    /// The deployment's genesis identity: the parent of its first block.
    async fn genesis(&self) -> [u8; 32] {
        let answer: serde_json::Value = Self::client()
            .post(format!("http://{}", self.rpc))
            .json(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "vera_getLightBlock", "params": ["0x1"]
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let parent = answer["result"]["parent_hash"]
            .as_str()
            .unwrap_or_else(|| panic!("no first block: {answer}"));
        hex::decode(parent.trim_start_matches("0x"))
            .unwrap()
            .try_into()
            .expect("32 bytes")
    }

    /// `user`'s verified create of a document called `name`, with the
    /// credential `credential` makes for the document it creates.
    async fn create<C, F>(&self, user: &User, name: &str, credential: C) -> Attempt
    where
        C: FnOnce(String) -> F,
        F: std::future::Future<Output = SignerCredential>,
    {
        let schema = self.node.get_collection("Users").unwrap().unwrap();
        let crdts = storage::Crdts::default();
        let input = json!({ "name": name });
        let signed = db::block::builder::ClientCreateSignatures::from_input(
            &crdts,
            &schema,
            &input,
            &user.blocks,
        )
        .unwrap();
        let graph = db::block::builder::VerifiedClientCreate::verify(
            &crdts,
            &schema,
            &input,
            signed.evidence(),
        )
        .unwrap();
        assert_eq!(graph.signer_did(), user.did.as_str());
        let request = QueryRequest::new(
            "mutation($input:UsersMutationInputArg!) { create_Users(input:$input) {_docID} }",
        )
        .with_variables(json!({ "input": input }))
        .with_identity(Some(user.did.clone()));
        let doc = graph.doc_id().to_owned();
        let credential = credential(doc.clone()).await;
        let txn = self.node.begin_transaction(false).await.unwrap();
        let response = self
            .node
            .execute_verified_create_in_txn(request, &txn, &graph, credential)
            .await;
        Attempt { doc, txn, response }
    }

    /// `user`'s delegation bound to the one command `action` on `doc`.
    async fn bound(&self, user: &User, doc: &str, action: PolicyAction<'_>, nonce: u8) -> String {
        let digest = request_digest(&PolicyCommand {
            policy_id: &self.policy,
            resource: "users",
            object_id: doc,
            action,
        })
        .expect("digest");
        request_delegation(
            &user.delegations,
            &self.audience,
            &RequestBinding {
                digest,
                genesis_id: self.genesis().await,
                nonce: [nonce; 24],
            },
            LIFETIME,
        )
        .await
        .expect("delegation")
    }

    /// The owner the chain itself records for `doc`.
    async fn owner_on_chain(&self, doc: &str) -> Option<String> {
        self.node
            .vera_acp()
            .expect("vera.rs document ACP")
            .get_doc_owner(&self.policy, "users", doc)
            .await
            .expect("owner")
            .map(|owner| owner.to_string())
    }

    /// The attempt was refused for want of authority, and the chain was
    /// sent nothing.
    async fn refused(&self, attempt: Attempt) {
        let Attempt { doc, txn, response } = attempt;
        assert_eq!(
            response.errors.first().and_then(|error| error.code()),
            Some(WRITE_AUTHORITY_UNAVAILABLE_ERROR_CODE),
            "{:?}",
            response.errors
        );
        assert!(self.node.commit_transaction(&txn).await.is_err());
        assert_eq!(self.owner_on_chain(&doc).await, None);
    }
}

/// A user whose key the node does not hold creates a document through a
/// verified write under its own delegation, and the chain names it owner.
#[test]
fn a_verified_create_under_the_signers_delegation_registers_the_signer() {
    workers::block_on(async {
        let on = Deployed::start(9411).await;
        let user = User::new();

        on.refused(
            on.create(&user, "unowned", |_| async { SignerCredential::None })
                .await,
        )
        .await;
        // A delegation the user gave another worker.
        let elsewhere = caller_delegation(
            &user.delegations,
            &acp::DelegationAudience {
                worker: user.did.to_string(),
                deployment: on.audience.deployment,
            },
            LIFETIME,
        )
        .await
        .expect("delegation");
        on.refused(
            on.create(&user, "elsewhere", |_| async {
                SignerCredential::Delegation(elsewhere)
            })
            .await,
        )
        .await;
        // A delegation somebody else signed for this worker.
        let stranger = User::new();
        let foreign = caller_delegation(&stranger.delegations, &on.audience, LIFETIME)
            .await
            .expect("delegation");
        on.refused(
            on.create(&user, "foreign", |_| async {
                SignerCredential::Delegation(foreign)
            })
            .await,
        )
        .await;

        let delegation = caller_delegation(&user.delegations, &on.audience, LIFETIME)
            .await
            .expect("delegation");
        let Attempt { doc, txn, response } = on
            .create(&user, "delegated", |_| async {
                SignerCredential::Delegation(delegation)
            })
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(on.owner_on_chain(&doc).await, None);
        on.node.commit_transaction(&txn).await.expect("commit");
        assert_eq!(
            on.owner_on_chain(&doc).await.as_deref(),
            Some(user.did.as_str())
        );
        assert!(chain_reads(&on.node, &user.did, &on.policy, &doc).await);
        assert!(
            !chain_reads(
                &on.node,
                &identity::Did::new(STRANGER).unwrap(),
                &on.policy,
                &doc
            )
            .await
        );
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// The same create under a delegation bound to the registration of exactly
/// the document it makes. That delegation covers no other document, is no
/// plain delegation, and does not authenticate an HTTP caller.
#[test]
fn a_verified_create_under_a_request_bound_delegation_registers_the_signer() {
    workers::block_on(async {
        let on = Deployed::start(9412).await;
        let user = User::new();

        // Bound to the registration of another document: refused here,
        // before anything reaches the chain.
        let other = on
            .bound(&user, "bae-another", PolicyAction::RegisterObject, 1)
            .await;
        on.refused(
            on.create(&user, "other", |_| async {
                SignerCredential::RequestBound(other.clone())
            })
            .await,
        )
        .await;
        // A plain delegation is not taken as one bound to the request.
        let plain = caller_delegation(&user.delegations, &on.audience, LIFETIME)
            .await
            .expect("delegation");
        on.refused(
            on.create(&user, "plain", |_| async {
                SignerCredential::RequestBound(plain)
            })
            .await,
        )
        .await;
        // One bound to the request is not taken as a plain delegation.
        on.refused(
            on.create(&user, "misfiled", |doc| {
                let (on, user) = (&on, &user);
                async move {
                    SignerCredential::Delegation(
                        on.bound(user, &doc, PolicyAction::RegisterObject, 2).await,
                    )
                }
            })
            .await,
        )
        .await;

        let mut presented = String::new();
        let Attempt { doc, txn, response } = on
            .create(&user, "bound", |doc| {
                let presented = &mut presented;
                let (on, user) = (&on, &user);
                async move {
                    *presented = on.bound(user, &doc, PolicyAction::RegisterObject, 3).await;
                    SignerCredential::RequestBound(presented.clone())
                }
            })
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        on.node.commit_transaction(&txn).await.expect("commit");
        assert_eq!(
            on.owner_on_chain(&doc).await.as_deref(),
            Some(user.did.as_str())
        );

        // The token that registered the document logs nobody in.
        let knocked = Deployed::client()
            .post(format!("http://{}/api/v0/graphql", on.http))
            .header("Authorization", format!("Bearer {presented}"))
            .json(
                &json!({ "query": "mutation { add_Users(input: {name: \"other\"}) { _docID } }" }),
            )
            .send()
            .await
            .unwrap();
        assert!(
            matches!(knocked.status().as_u16(), 401 | 403),
            "{}",
            knocked.status()
        );
        // And it grants nothing on the document it registered.
        let granting = defra_core::RequestContext::default()
            .with_identity(Some(user.did.to_string()))
            .with_request_delegation(user.did.to_string(), presented);
        let refused = on
            .node
            .add_dac_actor_relationship_as(&granting, "Users", &doc, "reader", STRANGER)
            .await;
        assert!(refused.is_err(), "{refused:?}");
        assert!(
            !chain_reads(
                &on.node,
                &identity::Did::new(STRANGER).unwrap(),
                &on.policy,
                &doc
            )
            .await
        );
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A grant staged in the same verified transaction, each command under a
/// delegation bound to it, reaches the chain after the registration.
#[test]
fn a_grant_staged_in_a_verified_transaction_lands_after_the_registration() {
    workers::block_on(async {
        let on = Deployed::start(9413).await;
        let user = User::new();
        let reader = identity::Did::new(STRANGER).unwrap();
        let subject = SubjectRef::Actor(reader.to_string());
        let grant = |relation: &'static str| PolicyAction::SetRelationship {
            relation,
            subject: PolicySubject::Actor(&subject),
        };

        let Attempt { doc, txn, response } = on
            .create(&user, "shared", |doc| {
                let (on, user) = (&on, &user);
                async move {
                    SignerCredential::RequestBound(
                        on.bound(user, &doc, PolicyAction::RegisterObject, 1).await,
                    )
                }
            })
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let acting = |token: String| {
            defra_core::RequestContext::default()
                .with_identity(Some(user.did.to_string()))
                .with_request_delegation(user.did.to_string(), token)
        };
        let stage = |context: defra_core::RequestContext| {
            let (on, txn, doc, reader) = (&on, &txn, &doc, &reader);
            async move {
                on.node
                    .add_dac_actor_relationship_change_for_in_txn(
                        txn,
                        &context,
                        "Users",
                        doc,
                        "reader",
                        reader.as_str(),
                    )
                    .await
            }
        };
        // A delegation bound to a grant of another relation is refused by
        // the node, and nothing is staged.
        let writer = on.bound(&user, &doc, grant("writer"), 2).await;
        assert!(matches!(
            stage(acting(writer)).await,
            Err(defra_node::Error::Acp(
                acp::Error::OwedAuthorityUnavailable { .. }
            ))
        ));
        let staged = stage(acting(on.bound(&user, &doc, grant("reader"), 3).await)).await;
        assert!(
            matches!(
                staged,
                Ok(RelationshipChange::Staged {
                    authority_checked: true,
                    ..
                })
            ),
            "{staged:?}"
        );
        assert_eq!(on.owner_on_chain(&doc).await, None);
        on.node.commit_transaction(&txn).await.expect("commit");

        // The chain takes a grant from an object's owner only, so the
        // registration was there when the grant arrived.
        assert_eq!(
            on.owner_on_chain(&doc).await.as_deref(),
            Some(user.did.as_str())
        );
        assert!(chain_reads(&on.node, &reader, &on.policy, &doc).await);
        let vera = on.node.vera_acp().expect("vera.rs document ACP").clone();
        assert!(vera
            .check_named_permission(&reader, "reader", &on.policy, "users", &doc)
            .await
            .expect("relation check"));
        assert!(!vera
            .check_named_permission(&reader, "writer", &on.policy, "users", &doc)
            .await
            .expect("relation check"));
        drop(vera);
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
