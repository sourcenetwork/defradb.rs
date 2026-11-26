//! Owner lookups inside a transaction on a node's own Vera deployment: a
//! document registered before the transaction, one created in it, and one
//! whose registration is still owed all answer, and an owner the chain no
//! longer confirms at the commit conflicts the commit. Through the Node API
//! and through `POST /api/v0/acp/document/is-owner` with `x-defradb-tx`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use acp::DocumentACP as _;
use identity::{Did, Identity as _, RawIdentity};
use serde_json::{json, Value};
use vera::vera_rs::{DelegationSigner, Ed25519Signer};
use vera::VeraProvider as _;

use super::{create_user, created, starting_with, users, EmbeddedVeraActor, Node, POLICY};

/// A node whose Vera actor is `owner`, serving HTTP on `http`, with a
/// policy-bound `Users` collection.
struct Home {
    node: Node,
    owner: RawIdentity,
    signer: Arc<dyn DelegationSigner>,
    policy: String,
    http: SocketAddr,
    rpc: SocketAddr,
    _directory: tempfile::TempDir,
}

fn key() -> RawIdentity {
    RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap()
}

fn did(identity: &RawIdentity) -> Did {
    identity.did().unwrap()
}

impl Home {
    async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let owner = key();
        let signer: Arc<dyn DelegationSigner> =
            Arc::new(Ed25519Signer::from_slice(&owner.private_key_bytes()).unwrap());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let http = listener.local_addr().unwrap();
        let node = starting_with(
            directory.path(),
            EmbeddedVeraActor::Signer(Arc::clone(&signer)),
            Vec::new(),
            Some(([127, 0, 0, 1], 0).into()),
            |builder| builder.with_http(defra_node::HttpConfig::with_listener(listener).unwrap()),
        )
        .await
        .expect("node");
        let rpc = node.embedded_vera_rpc_addr().expect("bound RPC listener");
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        assert_eq!(vera.self_did(), Some(did(&owner).to_string()));
        let policy = vera
            .add_policy(did(&owner).as_str(), POLICY, None)
            .await
            .expect("policy");
        node.add_collection(&defra_core::RequestContext::ambient(), &users(&policy))
            .await
            .expect("collection");
        Self {
            node,
            owner,
            signer,
            policy,
            http,
            rpc,
            _directory: directory,
        }
    }

    /// The owner `asker` is told of `doc` inside `txn`, and whether it reads it.
    async fn owner_in(
        &self,
        txn: &query::TransactionHandle,
        asker: &Did,
        doc: &str,
    ) -> (bool, Option<Did>) {
        let rows = self
            .node
            .observe_document_owners_in_txn(txn, "Users", &[doc.to_owned()], Some(asker.clone()))
            .await
            .expect("owner observation");
        (rows[0].visible, rows[0].owner.clone())
    }

    fn client() -> net::http::Client {
        net::http::Client::builder(edge::testing::edge())
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap()
    }

    fn bearer(&self, identity: &RawIdentity) -> String {
        let token = identity::new_token(
            identity,
            Duration::from_secs(120),
            Some(self.http.to_string()),
            None,
        )
        .unwrap();
        format!("Bearer {}", String::from_utf8(token).unwrap())
    }

    async fn begin(&self, caller: &RawIdentity) -> String {
        let response = Self::client()
            .post(format!("http://{}/api/v0/tx", self.http))
            .header("Authorization", self.bearer(caller))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{}", response.status());
        let body: Value = response.json().await.unwrap();
        body["id"].to_string()
    }

    async fn is_owner(&self, caller: &RawIdentity, tx: &str, doc: &str) -> Value {
        let response = Self::client()
            .post(format!("http://{}/api/v0/acp/document/is-owner", self.http))
            .header("Authorization", self.bearer(caller))
            .header("x-defradb-tx", tx)
            .json(&json!({ "collection": "Users", "doc_ids": [doc] }))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert!(status.is_success(), "{status}: {body}");
        body["documents"][0].clone()
    }

    async fn create_in(&self, caller: &RawIdentity, tx: &str, name: &str) -> String {
        let response = Self::client()
            .post(format!("http://{}/api/v0/graphql", self.http))
            .header("Authorization", self.bearer(caller))
            .header("x-defradb-tx", tx)
            .json(&json!({
                "query": format!("mutation {{ add_Users(input: {{name: \"{name}\"}}) {{ _docID }} }}")
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

    async fn commit(&self, caller: &RawIdentity, tx: &str) -> u16 {
        Self::client()
            .post(format!("http://{}/api/v0/tx/{tx}", self.http))
            .header("Authorization", self.bearer(caller))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    /// A document created, outside any transaction, by somebody the node
    /// cannot act for: its registration waits for authority, still owed.
    async fn owed_by(&self, creator: &RawIdentity) -> String {
        let doc = created(
            self.node
                .execute_request(create_user("owed", &did(creator)))
                .await,
        );
        assert_eq!(
            self.node
                .vera_acp()
                .unwrap()
                .get_doc_owner(&self.policy, "users", &doc)
                .await
                .expect("owner"),
            None,
            "the chain does not hold the registration"
        );
        doc
    }

    /// The owner archives `doc` on the chain, through a client of its own:
    /// nothing on the node under test sends it, as a change made from
    /// another device is not.
    async fn archive(&self, doc: &str) {
        let trust = self.node.embedded_vera_trust().expect("deployment").clone();
        let client = self._directory.path().join(format!("client-{doc}"));
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(client.join("keys"), b"test-password").expect("keyring"),
        );
        let worker = vera::vera_rs::NativeWorker::open(
            &client.join("worker"),
            keyring.as_ref(),
            trust.chain_id,
        )
        .expect("worker");
        let provider = vera::VeraRsProvider::new_with_signer(
            format!("http://{}", self.rpc),
            &trust.consensus_key,
            vera::vera_rs::GenesisSource::Unnamed,
            Arc::clone(&self.signer),
            worker,
            &vera::AcpTuning::default(),
            None,
            &edge::testing::edge(),
        )
        .await
        .expect("client");
        let owner = did(&self.owner);
        let token = provider
            .create_bearer_token(owner.as_str())
            .await
            .expect("token");
        provider
            .archive_object(&token, &self.policy, "users", doc)
            .await
            .expect("archive");
    }
}

#[test]
fn an_owner_lookup_in_a_transaction_answers_on_an_embedded_vera_node() {
    workers::block_on(async {
        let home = Home::start().await;
        let owner = did(&home.owner);
        let creator = key();
        let registered = created(
            home.node
                .execute_request(create_user("registered", &owner))
                .await,
        );
        let owed = home.owed_by(&creator).await;

        let txn = home.node.begin_transaction(false).await.unwrap();
        let fresh = created(
            home.node
                .execute_request_in_txn(create_user("fresh", &owner), &txn)
                .await,
        );
        assert_eq!(home.owner_in(&txn, &owner, &registered).await, (true, Some(owner.clone())));
        assert_eq!(home.owner_in(&txn, &owner, &fresh).await, (true, Some(owner.clone())));
        assert_eq!(
            home.owner_in(&txn, &did(&creator), &owed).await,
            (true, Some(did(&creator)))
        );
        home.node.commit_transaction(&txn).await.expect("commit");

        let txn = home.node.begin_transaction(false).await.unwrap();
        assert_eq!(home.owner_in(&txn, &owner, &registered).await, (true, Some(owner.clone())));
        created(
            home.node
                .execute_request_in_txn(create_user("decided", &owner), &txn)
                .await,
        );
        home.archive(&registered).await;
        let error = home.node.commit_transaction(&txn).await.unwrap_err();
        assert!(
            matches!(error, query::TransactionError::Conflict(ref message) if message.contains("owner changed")),
            "{error:?}"
        );
        home.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

#[test]
fn the_is_owner_route_answers_in_a_transaction_on_an_embedded_vera_node() {
    workers::block_on(async {
        let home = Home::start().await;
        let owner = did(&home.owner);
        let creator = key();
        let registered = created(
            home.node
                .execute_request(create_user("registered", &owner))
                .await,
        );
        let owed = home.owed_by(&creator).await;

        let tx = home.begin(&home.owner).await;
        let fresh = home.create_in(&home.owner, &tx, "fresh").await;
        for doc in [&registered, &fresh] {
            let row = home.is_owner(&home.owner, &tx, doc).await;
            assert_eq!(row["visible"], true, "{row}");
            assert_eq!(row["is_owner"], true, "{row}");
            assert_eq!(row["owner_registered"], true, "{row}");
        }
        let row = home.is_owner(&home.owner, &tx, &owed).await;
        assert_eq!(
            row["visible"], false,
            "another creator's owed document: {row}"
        );
        assert_eq!(row["is_owner"], false, "{row}");
        assert_eq!(home.commit(&home.owner, &tx).await, 200);

        let tx = home.begin(&creator).await;
        let row = home.is_owner(&creator, &tx, &owed).await;
        assert_eq!(row["visible"], true, "{row}");
        assert_eq!(
            row["is_owner"], true,
            "the owed registration names its creator: {row}"
        );
        assert_eq!(home.commit(&creator, &tx).await, 200);

        let tx = home.begin(&home.owner).await;
        let row = home.is_owner(&home.owner, &tx, &registered).await;
        assert_eq!(row["is_owner"], true, "{row}");
        home.create_in(&home.owner, &tx, "decided").await;
        home.archive(&registered).await;
        assert_eq!(
            home.commit(&home.owner, &tx).await,
            409,
            "the owner changed"
        );
        home.node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
