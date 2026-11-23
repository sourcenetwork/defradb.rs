//! A client's HTTP transaction on a node whose document ACP is its own Vera
//! node: relationship changes are staged in it under the client's
//! delegation and reach the chain after the commit (#3080).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use acp::{DocumentACP as _, RemotePolicies as _};
use defra_node::{EmbeddedVeraActor, EmbeddedVeraConfig, EmbeddedVeraDeployment, Node};
use serde_json::{json, Value};
use vera::vera_rs::{caller_delegation, DelegationSigner, Ed25519Signer};

use super::{chain_reads, now, users, ACCOUNT_KEY, POLICY, STRANGER};

const LIFETIME: Duration = Duration::from_secs(120);

struct Home {
    node: Node,
    base: String,
    policy: String,
    audience: acp::DelegationAudience,
    client: net::http::Client,
    _directory: tempfile::TempDir,
}

/// A client that keeps its key and signs in with its delegation.
struct Client {
    did: identity::Did,
    signer: Ed25519Signer,
}

impl Client {
    fn new() -> Self {
        let key =
            identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let signer = Ed25519Signer::from_slice(&key.private_key_bytes()).expect("key");
        Self {
            did: identity::Did::new(signer.did()).unwrap(),
            signer,
        }
    }
}

impl Home {
    async fn start(chain_id: u64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(directory.path().join("keys"), b"test-password")
                .expect("keyring"),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address: SocketAddr = listener.local_addr().unwrap();
        let node = Node::builder()
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
                rpc_listen: None,
                worker_dir: directory.path().join("worker"),
                keyring,
                tuning: vera::AcpTuning::default(),
            })
            .build()
            .await
            .expect("node");
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
        let client = net::http::Client::builder(edge::testing::edge())
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        Self {
            node,
            base: format!("http://{address}/api/v0"),
            policy,
            audience,
            client,
            _directory: directory,
        }
    }

    async fn auth(&self, client: &Client) -> String {
        let delegation = caller_delegation(&client.signer, &self.audience, LIFETIME)
            .await
            .expect("delegation");
        format!("Bearer {delegation}")
    }

    async fn begin(&self, auth: &str) -> String {
        let begun = self
            .client
            .post(format!("{}/tx", self.base))
            .header("Authorization", auth)
            .send()
            .await
            .unwrap();
        assert!(begun.status().is_success(), "{}", begun.status());
        let body: Value = begun.json().await.unwrap();
        body["id"].as_str().unwrap().to_string()
    }

    async fn finish(&self, auth: &str, tx: &str, commit: bool) -> u16 {
        let url = format!("{}/tx/{tx}", self.base);
        let request = if commit {
            self.client.post(url)
        } else {
            self.client.delete(url)
        };
        request
            .header("Authorization", auth)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn create(&self, auth: &str, tx: &str) -> String {
        let response = self
            .client
            .post(format!("{}/graphql", self.base))
            .header("Authorization", auth)
            .header("x-defradb-tx", tx)
            .json(&json!({
                "query": "mutation { add_Users(input: {name: \"shared\"}) { _docID } }"
            }))
            .send()
            .await
            .unwrap();
        let body: Value = response.json().await.unwrap();
        body["data"]["add_Users"][0]["_docID"]
            .as_str()
            .unwrap_or_else(|| panic!("no document: {body}"))
            .to_owned()
    }

    async fn grant(&self, auth: &str, tx: &str, doc: &str, reader: &str) -> (u16, Value) {
        let response = self
            .client
            .post(format!("{}/acp/document/relationship", self.base))
            .header("Authorization", auth)
            .header("x-defradb-tx", tx)
            .json(&json!({
                "collection": "Users",
                "doc_id": doc,
                "relation": "reader",
                "target_actor": reader,
            }))
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    async fn owed_by(&self, did: &identity::Did) -> usize {
        let context = defra_core::RequestContext::default().with_identity(Some(did.to_string()));
        self.node
            .owed_acp_as(&context, did.as_str())
            .await
            .unwrap()
            .len()
    }

    async fn owner_on_chain(&self, doc: &str) -> Option<String> {
        self.node
            .vera_acp()
            .expect("vera.rs document ACP")
            .get_doc_owner(&self.policy, "users", doc)
            .await
            .expect("owner")
            .map(|owner| owner.to_string())
    }
}

/// Wait up to a minute for `holds`.
async fn settles<F, Fut>(what: &str, mut holds: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while !holds().await {
        assert!(std::time::Instant::now() < deadline, "{what}");
        edge::testing::edge()
            .sleep(Duration::from_millis(200))
            .await;
    }
}

/// A client creates a document and grants a reader in one HTTP transaction.
/// The commit stands, the chain records the client as owner and then the
/// grant, and the grantee reads.
#[test]
fn a_client_transaction_that_creates_and_grants_commits_and_the_grantee_reads() {
    workers::block_on(async {
        let home = Home::start(9431).await;
        let owner = Client::new();
        let reader = Client::new();
        let auth = home.auth(&owner).await;

        let tx = home.begin(&auth).await;
        let doc = home.create(&auth, &tx).await;
        let (status, granted) = home.grant(&auth, &tx, &doc, reader.did.as_str()).await;
        assert_eq!(status, 200, "{granted}");
        assert_eq!(granted["Staged"]["AuthorityChecked"], true, "{granted}");
        assert_eq!(home.finish(&auth, &tx, true).await, 200);

        settles("the grant did not reach the chain", || async {
            chain_reads(&home.node, &reader.did, &home.policy, &doc).await
        })
        .await;
        assert_eq!(
            home.owner_on_chain(&doc).await.as_deref(),
            Some(owner.did.as_str())
        );
        assert_eq!(home.owed_by(&owner.did).await, 0);
        home.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// A grant on a document its caller does not own is refused before the
/// commit and owes nothing; a grant staged in a transaction that is then
/// discarded leaves nothing owed either, and the chain never holds it.
#[test]
fn a_refused_or_discarded_grant_owes_nothing() {
    workers::block_on(async {
        let home = Home::start(9432).await;
        let owner = Client::new();
        let auth = home.auth(&owner).await;
        let tx = home.begin(&auth).await;
        let doc = home.create(&auth, &tx).await;
        assert_eq!(home.finish(&auth, &tx, true).await, 200);
        settles("the registration did not reach the chain", || async {
            home.owner_on_chain(&doc).await.as_deref() == Some(owner.did.as_str())
        })
        .await;

        let stranger = Client::new();
        let stranger_auth = home.auth(&stranger).await;
        let tx = home.begin(&stranger_auth).await;
        let (status, refused) = home.grant(&stranger_auth, &tx, &doc, STRANGER).await;
        assert_eq!(status, 401, "{refused}");
        assert_eq!(home.finish(&stranger_auth, &tx, true).await, 200);
        assert_eq!(home.owed_by(&stranger.did).await, 0);

        let tx = home.begin(&auth).await;
        let (status, staged) = home.grant(&auth, &tx, &doc, STRANGER).await;
        assert_eq!(status, 200, "{staged}");
        assert!(staged["Staged"].is_object(), "{staged}");
        assert_eq!(home.finish(&auth, &tx, false).await, 200);
        assert_eq!(home.owed_by(&owner.did).await, 0);

        let stranger_did = identity::Did::new(STRANGER).unwrap();
        assert!(!chain_reads(&home.node, &stranger_did, &home.policy, &doc).await);
        home.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
