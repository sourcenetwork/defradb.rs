//! Verified writes against a real chain: the node acts for a signer whose
//! key it does not hold under the signer's delegation, plain or bound to the
//! one command the write sends, and the chain records the signer as owner.
//! A delegation bound to one command is made from what the node reports
//! alone: its delegation audience names the deployment's genesis identity.

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
    caller_delegation, request_delegation, DelegationSigner, Ed25519Signer, RequestBinding,
};
use vera::{PolicyAction, PolicyCommand, PolicySubject, SubjectRef};

use super::rpc_port::on_free_rpc_port;
use super::{chain_reads, now, users, ACCOUNT_KEY, POLICY, STRANGER};

pub(super) const LIFETIME: Duration = Duration::from_secs(120);

/// A node on its own Vera deployment, its HTTP API served on loopback and,
/// unless the test says otherwise, the deployment's JSON-RPC methods too.
pub(super) struct Deployed {
    pub(super) node: Node,
    pub(super) http: SocketAddr,
    pub(super) rpc: Option<SocketAddr>,
    pub(super) policy: String,
    pub(super) audience: acp::DelegationAudience,
    pub(super) _directory: tempfile::TempDir,
}

/// A user who signs its own blocks and delegations, and whose key the node
/// never sees.
pub(super) struct User {
    pub(super) did: identity::Did,
    pub(super) blocks: SigningConfig,
    pub(super) delegations: Ed25519Signer,
}

impl User {
    pub(super) fn new() -> Self {
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
pub(super) struct Attempt {
    pub(super) doc: String,
    pub(super) txn: defra_node::TransactionHandle,
    pub(super) response: QueryResponse,
}

impl Deployed {
    pub(super) async fn start(chain_id: u64) -> Self {
        Self::start_serving(chain_id, true).await
    }

    async fn start_serving(chain_id: u64, serve_rpc: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(directory.path().join("keys"), b"test-password")
                .expect("keyring"),
        );
        let vera = directory.path().join("vera");
        let worker = directory.path().join("worker");
        let start = |rpc_listen: Option<SocketAddr>| {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let http = listener.local_addr().unwrap();
            let builder = Node::builder()
                .enable_signing()
                .with_http(defra_node::HttpConfig::with_listener(listener).unwrap())
                .with_embedded_vera(EmbeddedVeraConfig {
                    data_dir: vera.clone(),
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
                    rpc_listen,
                    worker_dir: worker.clone(),
                    keyring: Arc::clone(&keyring),
                    tuning: vera::AcpTuning::default(),
                });
            async move { builder.build().await.map(|node| (node, http)) }
        };
        let ((node, http), rpc) = if serve_rpc {
            let (started, rpc) = on_free_rpc_port(|rpc| start(Some(rpc))).await;
            (started, Some(rpc))
        } else {
            (start(None).await.expect("node"), None)
        };
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let admin = vera.self_did().expect("actor DID");
        let policy = vera.add_policy(&admin, POLICY, None).await.expect("policy");
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

    /// The deployment's genesis identity as the chain's own JSON-RPC
    /// answers it, unverified: the parent of its first block.
    async fn genesis_on_chain(&self) -> [u8; 32] {
        let rpc = self.rpc.expect("the deployment serves JSON-RPC");
        let answer: serde_json::Value = Self::client()
            .post(format!("http://{rpc}"))
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
    pub(super) async fn create<C, F>(&self, user: &User, name: &str, credential: C) -> Attempt
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

    /// The delegation audience the node reports over its HTTP API.
    async fn reported_audience(&self) -> acp::DelegationAudience {
        let status: acp::LightClientStatus = Self::client()
            .get(format!("http://{}/api/v0/acp/status", self.http))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        status
            .delegation
            .expect("a vera.rs node names its delegation audience")
    }

    /// `user`'s delegation bound to the one command `action` on `doc`.
    pub(super) async fn bound(
        &self,
        user: &User,
        doc: &str,
        action: PolicyAction<'_>,
        nonce: u8,
    ) -> String {
        self.bound_for(user, doc, action, nonce, LIFETIME).await
    }

    /// `user`'s delegation bound to `action` on `doc` on the deployment
    /// `audience` names, made from `audience` and the command alone.
    async fn bound_to(
        &self,
        audience: &acp::DelegationAudience,
        user: &User,
        doc: &str,
        action: PolicyAction<'_>,
        nonce: u8,
        lifetime: Duration,
    ) -> String {
        let command = PolicyCommand {
            policy_id: &self.policy,
            resource: "users",
            object_id: doc,
            action,
        };
        let binding = RequestBinding::for_command(audience, &command, [nonce; 24])
            .expect("the node reports its genesis identity");
        request_delegation(&user.delegations, audience, &binding, lifetime)
            .await
            .expect("delegation")
    }

    /// [`Self::bound`], lasting `lifetime`.
    async fn bound_for(
        &self,
        user: &User,
        doc: &str,
        action: PolicyAction<'_>,
        nonce: u8,
        lifetime: Duration,
    ) -> String {
        self.bound_to(&self.audience, user, doc, action, nonce, lifetime)
            .await
    }

    /// The owner the chain itself records for `doc`.
    pub(super) async fn owner_on_chain(&self, doc: &str) -> Option<String> {
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
                ..on.audience.clone()
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
        // The genesis identity the node reports is the chain's.
        assert_eq!(on.audience.genesis_id, Some(on.genesis_on_chain().await));

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

/// A client that holds only delegations bound to one request each: those it
/// wrote with run out before the commit sends anything. The commit stands,
/// the node says what it still owes and why, and each owed command takes a
/// fresh delegation bound to it, the registration first.
#[test]
fn owed_commands_take_fresh_delegations_bound_to_them() {
    workers::block_on(async {
        let on = Deployed::start(9414).await;
        let user = User::new();
        let reader = identity::Did::new(STRANGER).unwrap();
        let subject = SubjectRef::Actor(reader.to_string());
        let grant = || PolicyAction::SetRelationship {
            relation: "reader",
            subject: PolicySubject::Actor(&subject),
        };
        let brief = Duration::from_secs(2);
        let caller =
            defra_core::RequestContext::default().with_identity(Some(user.did.to_string()));

        let Attempt { doc, txn, response } = on
            .create(&user, "owed", |doc| {
                let (on, user) = (&on, &user);
                async move {
                    SignerCredential::RequestBound(
                        on.bound_for(user, &doc, PolicyAction::RegisterObject, 1, brief)
                            .await,
                    )
                }
            })
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let granting = on.bound_for(&user, &doc, grant(), 2, brief).await;
        let staged = on
            .node
            .add_dac_actor_relationship_change_for_in_txn(
                &txn,
                &caller
                    .clone()
                    .with_request_delegation(user.did.to_string(), granting),
                "Users",
                &doc,
                "reader",
                reader.as_str(),
            )
            .await;
        assert!(
            matches!(staged, Ok(RelationshipChange::Staged { .. })),
            "{staged:?}"
        );
        edge::testing::edge().sleep(Duration::from_secs(4)).await;
        on.node.commit_transaction(&txn).await.expect("commit");
        assert_eq!(on.owner_on_chain(&doc).await, None);

        let owed = on
            .node
            .owed_acp_as(&caller, user.did.as_str())
            .await
            .expect("report");
        assert_eq!(owed.len(), 2, "{owed:?}");
        assert_eq!(owed[0].what, acp::OwedWhat::Registration);
        assert_eq!(owed[0].state, acp::OwedState::AwaitingAuthority);
        assert!(owed[0].reason.is_some(), "{owed:?}");
        assert_eq!(owed[1].state, acp::OwedState::Staged);
        assert_eq!(owed[1].held_by, Some(acp::OwedHold::Registration));
        assert!(matches!(
            on.node
                .await_owed_acp_as(&caller, &txn, Duration::from_secs(30))
                .await
                .expect("wait"),
            acp::OwedWait::Parked(entries) if entries.len() == 2
        ));
        let stranger =
            defra_core::RequestContext::default().with_identity(Some(STRANGER.to_string()));
        assert!(on
            .node
            .owed_acp_as(&stranger, user.did.as_str())
            .await
            .is_err());

        // A fresh delegation bound to the grant does not register the
        // document, and one bound to the registration does.
        let registration = owed[0].reference();
        let misfiled = on.bound(&user, &doc, grant(), 3).await;
        assert!(matches!(
            on.node
                .reauthorize_owed_acp_as(
                    &caller,
                    &registration,
                    SignerCredential::RequestBound(misfiled)
                )
                .await,
            Err(defra_node::Error::Acp(
                acp::Error::OwedAuthorityUnavailable { .. }
            ))
        ));
        assert_eq!(on.owner_on_chain(&doc).await, None);
        let fresh = on.bound(&user, &doc, PolicyAction::RegisterObject, 4).await;
        assert_eq!(
            on.node
                .reauthorize_owed_acp_as(
                    &caller,
                    &registration,
                    SignerCredential::RequestBound(fresh)
                )
                .await
                .expect("registration"),
            acp::OwedOutcome::Settled
        );
        assert_eq!(
            on.owner_on_chain(&doc).await.as_deref(),
            Some(user.did.as_str())
        );

        // The grant behind it was sent with its own delegation, which had
        // run out, and now waits on its own account.
        let owed = on
            .node
            .owed_acp_as(&caller, user.did.as_str())
            .await
            .expect("report");
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].state, acp::OwedState::AwaitingAuthority);
        assert_eq!(owed[0].held_by, None);
        assert!(!chain_reads(&on.node, &reader, &on.policy, &doc).await);
        let fresh = on.bound(&user, &doc, grant(), 5).await;
        assert_eq!(
            on.node
                .reauthorize_owed_acp_as(
                    &caller,
                    &owed[0].reference(),
                    SignerCredential::RequestBound(fresh)
                )
                .await
                .expect("grant"),
            acp::OwedOutcome::Settled
        );
        assert!(chain_reads(&on.node, &reader, &on.policy, &doc).await);
        assert_eq!(
            on.node
                .await_owed_acp_as(&caller, &txn, Duration::from_secs(30))
                .await
                .expect("wait"),
            acp::OwedWait::Settled
        );
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A client of a deployment that serves no JSON-RPC learns what to bind its
/// delegation to from the node's reported audience alone. One naming another
/// deployment's genesis identity is refused before anything is written; one
/// made from the audience registers the signer.
#[test]
fn a_client_binds_its_delegation_to_the_audience_the_node_reports() {
    workers::block_on(async {
        let on = Deployed::start_serving(9415, false).await;
        let user = User::new();
        let audience = on.reported_audience().await;
        assert_eq!(audience, on.audience);
        let genesis = audience
            .genesis_id
            .expect("the node knows its genesis identity");

        let elsewhere = acp::DelegationAudience {
            genesis_id: Some(genesis.map(|byte| byte ^ 0xff)),
            ..audience.clone()
        };
        on.refused(
            on.create(&user, "elsewhere", |doc| {
                let (on, user, elsewhere) = (&on, &user, &elsewhere);
                async move {
                    SignerCredential::RequestBound(
                        on.bound_to(
                            elsewhere,
                            user,
                            &doc,
                            PolicyAction::RegisterObject,
                            1,
                            LIFETIME,
                        )
                        .await,
                    )
                }
            })
            .await,
        )
        .await;

        let Attempt { doc, txn, response } = on
            .create(&user, "bound", |doc| {
                let (on, user, audience) = (&on, &user, &audience);
                async move {
                    SignerCredential::RequestBound(
                        on.bound_to(
                            audience,
                            user,
                            &doc,
                            PolicyAction::RegisterObject,
                            2,
                            LIFETIME,
                        )
                        .await,
                    )
                }
            })
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        on.node.commit_transaction(&txn).await.expect("commit");
        assert_eq!(
            on.owner_on_chain(&doc).await.as_deref(),
            Some(user.did.as_str())
        );
        assert!(chain_reads(&on.node, &user.did, &on.policy, &doc).await);
        on.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
