//! A caller whose key the server does not hold authenticates with its own
//! Vera delegation in `Authorization: Bearer`, verified by the node's
//! document ACP; a delegation that fails any check is refused.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde_json::{json, Value};
use vera::vera_rs::{
    caller_delegation, did_spelling, request_delegation, DelegationSigner, Ed25519Signer,
    RequestBinding,
};
use vera_crypto::jwt::DelegationScope;
use vera_delegation::bearer::create_scoped_bearer_token_at;

use super::{add_policy, policy_id, RunningNode};

const LIFETIME: Duration = Duration::from_secs(120);

/// A caller that keeps its key: the server holds none for it.
struct Caller {
    key: identity::RawIdentity,
    signer: Ed25519Signer,
}

impl Caller {
    fn new() -> Self {
        let key =
            identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let signer = Ed25519Signer::from_slice(&key.private_key_bytes()).expect("key");
        Self { key, signer }
    }

    fn did(&self) -> String {
        self.signer.did().to_owned()
    }

    async fn delegation(&self, audience: &acp::DelegationAudience) -> String {
        caller_delegation(&self.signer, audience, LIFETIME)
            .await
            .expect("delegation")
    }

    /// A delegation for `audience` in `scope`, issued at `issued`.
    async fn scoped(
        &self,
        audience: &acp::DelegationAudience,
        scope: DelegationScope,
        issued: u64,
    ) -> String {
        create_scoped_bearer_token_at(
            &self.signer,
            &did_spelling::wire(self.signer.did()),
            &audience.worker,
            audience.deployment,
            LIFETIME.as_secs(),
            scope,
            issued,
        )
        .await
        .expect("delegation")
    }

    /// The identity token DefraDB's own client sends for this caller.
    fn identity_token(&self, host: &str) -> String {
        let token = identity::new_token(
            &self.key,
            Duration::from_secs(120),
            Some(host.to_owned()),
            None,
        )
        .expect("identity token");
        String::from_utf8(token).unwrap()
    }
}

struct Server {
    node: RunningNode,
    base: String,
    audience: acp::DelegationAudience,
    client: net::http::Client,
}

impl Server {
    async fn start(root: &std::path::Path) -> Self {
        let node = RunningNode::start(root, "delegation").await;
        let policy = policy_id(&add_policy(&node, root).await);
        let client = net::http::Client::builder(edge::testing::edge())
            .no_proxy()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap();
        let base = format!("http://{}/api/v0", node.address);
        let added = client
            .post(format!("{base}/collections"))
            .body(format!(
                "type Users @policy(id: \"{policy}\", resource: \"users\") {{ name: String }}"
            ))
            .send()
            .await
            .unwrap();
        assert!(
            added.status().is_success(),
            "{}",
            added.text().await.unwrap()
        );
        let status: Value = client
            .get(format!("{base}/acp/status"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let audience = serde_json::from_value(status["delegation"].clone())
            .unwrap_or_else(|_| panic!("no delegation audience in {status}"));
        Self {
            node,
            base,
            audience,
            client,
        }
    }

    /// Sends `query` with `Authorization: Bearer {token}`.
    async fn graphql(&self, token: &str, query: &str) -> (u16, Value) {
        let response = self
            .client
            .post(format!("{}/graphql", self.base))
            .header("Authorization", format!("Bearer {token}"))
            .json(&json!({ "query": query }))
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

    async fn create(&self, token: &str, name: &str) -> (u16, Value) {
        self.graphql(
            token,
            &format!("mutation {{ add_Users(input: {{name: \"{name}\"}}) {{ _docID }} }}"),
        )
        .await
    }

    async fn names(&self, token: &str) -> Vec<String> {
        let (status, body) = self.graphql(token, "{ Users { name } }").await;
        assert_eq!(status, 200, "{body}");
        assert!(body.get("errors").is_none(), "{body}");
        let mut names: Vec<String> = body["data"]["Users"]
            .as_array()
            .unwrap_or_else(|| panic!("no Users in {body}"))
            .iter()
            .map(|user| user["name"].as_str().unwrap().to_owned())
            .collect();
        names.sort();
        names
    }

    /// Whether the caller of `token` owns `doc` and whether Vera holds an
    /// owner for it.
    async fn ownership(&self, token: &str, doc: &str) -> (bool, bool) {
        let auth = format!("Bearer {token}");
        let begun: Value = self
            .client
            .post(format!("{}/tx", self.base))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let tx = begun["id"]
            .as_u64()
            .unwrap_or_else(|| panic!("no transaction: {begun}"))
            .to_string();
        let response = self
            .client
            .post(format!("{}/acp/document/is-owner", self.base))
            .header("Authorization", &auth)
            .header("x-defradb-tx", &tx)
            .json(&json!({ "collection": "Users", "doc_ids": [doc] }))
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let body: Value = response.json().await.unwrap();
        let discarded = self
            .client
            .delete(format!("{}/tx/{tx}", self.base))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert!(discarded.status().is_success(), "{}", discarded.status());
        assert_eq!(status, 200, "{body}");
        let document = &body["documents"][0];
        (
            document["is_owner"].as_bool().unwrap_or(false),
            document["owner_registered"].as_bool().unwrap_or(false),
        )
    }

    /// Whether the server sent Vera the registration of `doc`.
    fn sent_registration(&self, doc: &str) -> bool {
        self.node
            .logs()
            .lines()
            .any(|line| line.contains("register_object send_tx completed") && line.contains(doc))
    }

    /// Waits up to a minute for `holds`.
    async fn settles<F, Fut>(&self, what: &str, mut holds: F)
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !holds().await {
            assert!(
                std::time::Instant::now() < deadline,
                "{what}: {}",
                self.node.logs()
            );
            nuclei::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// `token` with its issuer claim replaced by `issuer` and its signature kept.
fn reissued(token: &str, issuer: &str) -> String {
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let parts: Vec<&str> = token.split('.').collect();
    let mut claims: Value = serde_json::from_slice(&engine.decode(parts[1]).unwrap()).unwrap();
    claims["iss"] = Value::String(issuer.to_owned());
    let claims = engine.encode(serde_json::to_vec(&claims).unwrap());
    format!("{}.{claims}.{}", parts[0], parts[2])
}

/// A caller creates a document over HTTP under its own delegation: the
/// server registers it on Vera with the caller as owner, the caller reads it
/// back and another caller does not see it. A caller that presents only an
/// identity token gives the server nothing to register its document under.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.vera_http_delegation")]
async fn a_caller_the_server_holds_no_key_for_creates_documents_under_its_delegation() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start(root.path()).await;
    let caller = Caller::new();
    let stranger = Caller::new();
    let delegation = caller.delegation(&server.audience).await;

    let (status, created) = server.create(&delegation, "delegated").await;
    assert_eq!(status, 200, "{created}");
    assert!(created.get("errors").is_none(), "{created}");
    let delegated = created["data"]["add_Users"][0]["_docID"]
        .as_str()
        .unwrap_or_else(|| panic!("no document: {created}"))
        .to_owned();
    server
        .settles("the delegated document was not registered", || async {
            server.sent_registration(&delegated)
        })
        .await;
    assert_eq!(
        server.ownership(&delegation, &delegated).await,
        (true, true)
    );
    assert_eq!(server.names(&delegation).await, ["delegated"]);
    let stranger_delegation = stranger.delegation(&server.audience).await;
    assert!(server.names(&stranger_delegation).await.is_empty());

    let keyless = Caller::new();
    let identity = keyless.identity_token(&server.node.address.to_lowercase());
    let (status, created) = server.create(&identity, "undelegated").await;
    assert_eq!(status, 200, "{created}");
    let undelegated = created["data"]["add_Users"][0]["_docID"]
        .as_str()
        .unwrap_or_else(|| panic!("no document: {created}"))
        .to_owned();
    let refusal = format!(
        "failed to register ACP document {undelegated}: no authority to submit ACP changes as {}: \
         the node holds no key for it and the request carries no delegation",
        keyless.did()
    );
    server
        .settles("the registration was not refused", || async {
            server.node.logs().contains(&refusal)
        })
        .await;
    assert!(
        !server.sent_registration(&undelegated),
        "a document created under an identity token alone was registered: {}",
        server.node.logs()
    );
    assert_eq!(server.names(&delegation).await, ["delegated"]);
    server.node.stop().await;
}

/// Each delegation that fails a check is refused with 403 before the
/// request runs, never taken as an identity token instead.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.vera_http_delegation")]
async fn a_delegation_that_fails_a_check_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start(root.path()).await;
    let caller = Caller::new();
    let stranger = Caller::new();
    let audience = &server.audience;
    let elsewhere = acp::DelegationAudience {
        deployment: audience.deployment + 1,
        ..audience.clone()
    };
    let other_worker = acp::DelegationAudience {
        worker: stranger.did(),
        ..audience.clone()
    };
    let valid = caller.delegation(audience).await;
    let header = valid.split('.').next().unwrap().to_owned();
    let oversized = format!(
        "{header}.{}.{}",
        "A".repeat(16 * 1024),
        valid.rsplit('.').next().unwrap()
    );

    let refused = [
        (
            "expired",
            "it has expired",
            caller
                .scoped(audience, DelegationScope::PolicyCommands, now() - 3_600)
                .await,
        ),
        (
            "not yet valid",
            "it is not valid yet",
            caller
                .scoped(audience, DelegationScope::PolicyCommands, now() + 3_600)
                .await,
        ),
        (
            "another scope",
            "not a plain delegation for policy commands",
            caller
                .scoped(audience, DelegationScope::CreatePolicy, now())
                .await,
        ),
        (
            "bound to one request",
            "not a plain delegation for policy commands",
            request_delegation(
                &caller.signer,
                audience,
                &RequestBinding {
                    digest: [7; 32],
                    genesis_id: audience.genesis_id.unwrap_or([9; 32]),
                    nonce: [5; 24],
                },
                LIFETIME,
            )
            .await
            .expect("request delegation"),
        ),
        (
            "another deployment",
            "caller or deployment mismatch",
            caller.delegation(&elsewhere).await,
        ),
        (
            "another worker",
            "caller or deployment mismatch",
            caller.delegation(&other_worker).await,
        ),
        (
            "another issuer",
            "invalid signature",
            reissued(&valid, &stranger.did()),
        ),
        (
            "malformed",
            "payload decode",
            format!("{header}.not-claims.not-a-signature"),
        ),
        ("oversized", "token exceeds 16 KiB", oversized),
    ];
    for (what, reason, token) in refused {
        let (status, body) = server.create(&token, what).await;
        assert_eq!(status, 403, "{what} delegation: {body}");
        let error = body["error"].as_str().unwrap_or_default();
        assert!(
            error.starts_with("Token verification failed: ") && error.contains(reason),
            "{what} delegation was not refused as one: {body}"
        );
    }
    assert!(server.names(&valid).await.is_empty());
    assert!(server
        .names(&stranger.delegation(audience).await)
        .await
        .is_empty());
    server.node.stop().await;
}
