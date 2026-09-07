//! DefraClient - the main WASM client interface.
//!
//! Provides a high-level API for browser applications to interact with DefraDB.
//! This wraps the `db` crate's `DB` type with a JavaScript-friendly interface.

use std::sync::Arc;

use wasm_bindgen::prelude::*;

use db::{AutoCommitMutator, DbCollectionProvider, LensedAutoCommitFetcher, DB};
use events::Bus;
use query::runner::QueryRunner;
use storage::RegolithStore;

type WasmRunner = QueryRunner<LensedAutoCommitFetcher<RegolithStore>>;

use crate::bindings::{from_js, to_js, ClientConfig, CollectionInfo, FieldInfo};
use crate::error::{Result, WasmError};
use crate::identity::{ClientIdentity, SigningGuard};
use defra_core::browser_sync::BrowserSyncRelationship;

/// DefraDB client for browser applications.
///
/// This client wraps the core `db::DB` type and provides a simplified interface for:
/// - Schema management
/// - GraphQL queries and mutations
/// - Document storage with CRDT support
/// - Merkle proof verification
///
/// # Example (JavaScript)
///
/// ```javascript
/// import init, { DefraClient } from 'defra-wasm';
///
/// await init();
/// const client = await DefraClient.create({ storage: 'memory' });
///
/// await client.add_schema(`
///   type User {
///     name: String
///     email: String
///   }
/// `);
///
/// const collections = client.get_collections();
/// console.log(collections);
///
/// await client.close();
/// ```
#[wasm_bindgen]
pub struct DefraClient {
    db: Option<Arc<DB<RegolithStore>>>,
    runner: Option<WasmRunner>,
    event_bus: Arc<events::ChannelBus>,
    sync_task: Option<crate::sync::SyncTask>,
    identity: Option<ClientIdentity>,
    grants: Vec<BrowserSyncRelationship>,
    mutate_lock: futures::lock::Mutex<()>,
    closed: bool,
}

#[wasm_bindgen]
impl DefraClient {
    /// Create a new DefraDB client.
    ///
    /// # Configuration
    ///
    /// Pass a JavaScript object with:
    /// - `db_name`: the OPFS directory the store lives in
    /// - `db_name`: Database name (optional)
    ///
    /// # Example
    ///
    /// ```javascript
    /// const client = await DefraClient.create({ db_name: 'defradb' });
    /// ```
    #[wasm_bindgen(js_name = create)]
    pub async fn create(config: JsValue) -> std::result::Result<DefraClient, JsValue> {
        Self::new_impl(config).await.map_err(|e| e.into())
    }

    /// Add a GraphQL schema definition.
    ///
    /// Parses and validates the SDL, then registers the collections.
    ///
    /// # Example
    ///
    /// ```javascript
    /// await client.add_schema(`
    ///   type User {
    ///     name: String
    ///     email: String
    ///   }
    ///
    ///   type Post {
    ///     title: String
    ///     content: String
    ///   }
    /// `);
    /// ```
    #[wasm_bindgen]
    pub async fn add_schema(&mut self, sdl: &str) -> std::result::Result<JsValue, JsValue> {
        self.add_schema_impl(sdl).await.map_err(|e| e.into())
    }

    /// Execute a GraphQL query.
    ///
    /// # Example
    ///
    /// ```javascript
    /// const result = await client.query(`{
    ///   User {
    ///     name
    ///     email
    ///   }
    /// }`);
    /// ```
    #[wasm_bindgen]
    pub async fn query(&self, graphql: &str) -> std::result::Result<JsValue, JsValue> {
        self.query_impl(graphql).await.map_err(|e| e.into())
    }

    /// Execute a GraphQL mutation.
    #[wasm_bindgen]
    pub async fn mutate(&self, graphql: &str) -> std::result::Result<JsValue, JsValue> {
        self.mutate_impl(graphql).await.map_err(|e| e.into())
    }

    /// Author with a key this client holds, given as hex.
    ///
    /// Every block written afterwards is signed with it, and the key never
    /// leaves the tab: a server merging these blocks derives their owner from
    /// the signature and cannot produce one itself. Returns the DID of the key.
    ///
    /// # Example
    ///
    /// ```javascript
    /// const did = client.set_identity(privateKeyHex, 'ed25519');
    /// ```
    #[wasm_bindgen]
    pub fn set_identity(
        &mut self,
        private_key_hex: &str,
        key_type: &str,
    ) -> std::result::Result<String, JsValue> {
        self.ensure_open()?;
        let identity = ClientIdentity::from_private_key(private_key_hex, key_type)?;
        let did = identity.did().to_string();
        self.identity = Some(identity);
        Ok(did)
    }

    /// Grant these relations on every document this client authors, applied by
    /// the push that registers the document rather than by a call after it.
    ///
    /// Each entry is `{ relation, target }`, where `target` is an actor DID or
    /// `*` for everyone. Documents this client did not sign are pushed
    /// untouched — the node would refuse a grant on them.
    ///
    /// # Example
    ///
    /// ```javascript
    /// client.set_grants([{ relation: 'reader', target: '*' }]);
    /// ```
    #[wasm_bindgen]
    pub fn set_grants(&mut self, grants: JsValue) -> std::result::Result<(), JsValue> {
        self.ensure_open()?;
        self.grants = if grants.is_undefined() || grants.is_null() {
            Vec::new()
        } else {
            from_js(grants)?
        };
        Ok(())
    }

    /// The DID this client authors as, or `undefined` when it holds no key.
    #[wasm_bindgen]
    pub fn did(&self) -> Option<String> {
        self.identity.as_ref().map(|id| id.did().to_string())
    }

    /// A JWT proving possession of this client's key, for `audience` — the host
    /// of the server it will be sent to, which is what that server checks it
    /// against.
    ///
    /// `sync` mints one of these for itself, so this is for callers that need
    /// the token for a request of their own.
    #[wasm_bindgen]
    pub fn auth_token(&self, audience: Option<String>) -> std::result::Result<String, JsValue> {
        self.ensure_open()?;
        Ok(self
            .identity
            .as_ref()
            .ok_or(WasmError::Identity("client holds no key".into()))?
            .auth_token(audience)?)
    }

    /// Get information about all registered collections.
    ///
    /// Returns an array of collection info objects.
    #[wasm_bindgen]
    pub fn get_collections(&self) -> std::result::Result<JsValue, JsValue> {
        self.get_collections_impl().map_err(|e| e.into())
    }

    /// Persist pending data to OPFS.
    ///
    /// Call this to flush in-memory LevelDB data to the browser's
    /// Origin Private File System. Without calling persist, data only
    /// lives in memory and will be lost if the tab is closed.
    ///
    /// # Example
    ///
    /// ```javascript
    /// // Persist on visibility change (tab going to background)
    /// document.addEventListener('visibilitychange', () => {
    ///     if (document.visibilityState === 'hidden') {
    ///         client.persist();
    ///     }
    /// });
    ///
    /// // Or on a timer
    /// setInterval(() => client.persist(), 10000);
    /// ```
    #[wasm_bindgen]
    pub async fn persist(&self) -> std::result::Result<(), JsValue> {
        self.persist_impl().await.map_err(|e| e.into())
    }

    /// Start bidirectional synchronization with a DefraDB server.
    ///
    /// The optional token may be either a raw JWT or a `Bearer <JWT>` value.
    #[wasm_bindgen]
    pub async fn sync(
        &mut self,
        server_url: &str,
        auth_token: Option<String>,
    ) -> std::result::Result<(), JsValue> {
        self.sync_impl(server_url, auth_token)
            .await
            .map_err(Into::into)
    }

    /// Close the client and release resources.
    ///
    /// After closing, the client cannot be used.
    #[wasm_bindgen]
    pub async fn close(&mut self) -> std::result::Result<(), JsValue> {
        self.close_impl().await.map_err(|e| e.into())
    }
}

// Internal implementation methods
impl DefraClient {
    // The LevelDB-backed database and its mutator are not `Send`, since wasm
    // drops those bounds for the single-threaded browser runtime. Sharing them
    // by `Arc` is what the query runner expects, and there are no threads to
    // send them across.
    #[allow(clippy::arc_with_non_send_sync)]
    async fn new_impl(config: JsValue) -> Result<Self> {
        let config: ClientConfig = if config.is_undefined() || config.is_null() {
            ClientConfig::default()
        } else {
            from_js(config)?
        };

        // A regolith store on the origin-private filesystem, which is the
        // only filesystem this target has.
        let db_name = config.db_name.as_deref().unwrap_or("defradb");
        let store = RegolithStore::open_opfs(db_name)
            .await
            .map_err(|e| WasmError::Storage(format!("Failed to open store: {}", e)))?;

        // Create the database
        let event_bus = Arc::new(events::ChannelBus::new());
        let mut db = DB::new(store)
            .map_err(|e| WasmError::Storage(format!("Failed to create database: {}", e)))?;
        db.set_event_bus(event_bus.clone());

        // Load existing collections from storage
        db.load_collections()
            .await
            .map_err(|e| WasmError::Storage(format!("Failed to load collections: {}", e)))?;

        let db = Arc::new(db);

        let fetcher = LensedAutoCommitFetcher::new(Arc::clone(&db));
        let provider = DbCollectionProvider::new_arc(Arc::clone(&db));
        let mutator = Arc::new(AutoCommitMutator::new(Arc::clone(&db)));
        let runner = QueryRunner::with_provider(fetcher, provider)
            .with_mutator(mutator)
            .with_collection_truncator(db::DbCollectionTruncator::new_arc(Arc::clone(&db)));

        let identity = match (config.private_key.as_deref(), config.key_type.as_deref()) {
            (Some(private_key), key_type) => Some(ClientIdentity::from_private_key(
                private_key,
                key_type.unwrap_or("ed25519"),
            )?),
            (None, _) => None,
        };

        Ok(Self {
            db: Some(db),
            runner: Some(runner),
            event_bus,
            sync_task: None,
            identity,
            grants: Vec::new(),
            mutate_lock: futures::lock::Mutex::new(()),
            closed: false,
        })
    }

    fn ensure_open(&self) -> Result<&Arc<DB<RegolithStore>>> {
        if self.closed {
            return Err(WasmError::Closed);
        }
        self.db.as_ref().ok_or(WasmError::NotInitialized)
    }

    async fn persist_impl(&self) -> Result<()> {
        let db = self.ensure_open()?;
        db.store()
            .persist()
            .await
            .map_err(|e| WasmError::Storage(format!("Persist failed: {}", e)))?;
        Ok(())
    }

    async fn add_schema_impl(&mut self, sdl: &str) -> Result<JsValue> {
        let db = self.ensure_open()?;

        // Parse SDL into CollectionVersions using the query crate's parser
        let collections = query::sdl_parse::parse_sdl(sdl)
            .map_err(|e| WasmError::Schema(format!("Failed to parse SDL: {}", e)))?;

        schema::definition_validation::validate_new_collections(&collections)
            .map_err(|e| WasmError::Schema(format!("Failed to validate SDL: {}", e)))?;

        // Create each collection in the database
        let mut added = Vec::new();
        for collection in collections {
            let name = collection.name.clone();
            db.create_collection(collection).await.map_err(|e| {
                WasmError::Schema(format!("Failed to create collection '{}': {}", name, e))
            })?;
            added.push(name);
        }

        // Persist to OPFS so schema definitions survive page refresh
        self.persist_impl().await?;

        to_js(&serde_json::json!({
            "collections_added": added,
        }))
    }

    async fn query_impl(&self, graphql: &str) -> Result<JsValue> {
        self.ensure_open()?;

        if graphql.trim().is_empty() {
            return Err(WasmError::Query("Empty query string".to_string()));
        }

        let runner = self.runner.as_ref().ok_or(WasmError::NotInitialized)?;

        match runner.execute_query(graphql).await {
            Ok(result) => {
                let response = serde_json::json!({
                    "data": result,
                    "errors": [],
                });
                to_js(&response)
            }
            Err(e) => Err(WasmError::Query(e.to_string())),
        }
    }

    async fn mutate_impl(&self, graphql: &str) -> Result<JsValue> {
        self.ensure_open()?;

        if graphql.trim().is_empty() {
            return Err(WasmError::Query("Empty mutation string".to_string()));
        }

        // One mutation at a time. The signing config is a thread-local, so a
        // second mutation starting mid-flight would take it away from the
        // first, which would go on to write unsigned blocks.
        let _writing = self.mutate_lock.lock().await;
        let _signing = SigningGuard::install(self.identity.as_ref());
        let result = self
            .runner
            .as_ref()
            .ok_or(WasmError::NotInitialized)?
            .execute_mutation(graphql)
            .await;

        match result {
            Ok(data) => {
                let response = serde_json::json!({
                    "data": data,
                    "errors": [],
                });
                to_js(&response)
            }
            Err(e) => Err(WasmError::Query(e.to_string())),
        }
    }

    fn get_collections_impl(&self) -> Result<JsValue> {
        let db = self.ensure_open()?;

        // Get collection names from the database
        let names = db
            .list_collections()
            .map_err(|e| WasmError::Storage(format!("Failed to list collections: {}", e)))?;

        // Get each collection's info
        let mut collections = Vec::new();
        for name in names {
            if let Some(col) = db.get_collection(&name).map_err(|e| {
                WasmError::Storage(format!("Failed to get collection '{}': {}", name, e))
            })? {
                let schema = col.schema();
                collections.push(CollectionInfo {
                    name: schema.name.clone(),
                    schema_version_id: schema.version_id.clone(),
                    fields: schema.fields.iter().map(FieldInfo::from).collect(),
                });
            }
        }

        to_js(&collections)
    }

    async fn close_impl(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }

        self.stop_sync().await;
        self.event_bus.close();

        // Drop runner first — it holds Arc<DB> refs via fetcher, mutator, and provider
        self.runner = None;

        if let Some(db) = self.db.take() {
            match Arc::try_unwrap(db) {
                Ok(db) => {
                    db.close().await.map_err(|e| {
                        WasmError::Storage(format!("Failed to close database: {}", e))
                    })?;
                }
                Err(arc) => {
                    // Other references exist — persist to avoid data loss, then drop our ref
                    web_sys::console::warn_1(
                        &format!(
                            "Cannot close DB exclusively ({} refs), persisting before release",
                            Arc::strong_count(&arc)
                        )
                        .into(),
                    );
                    arc.store().persist().await.map_err(|e| {
                        WasmError::Storage(format!("Persist failed during close: {}", e))
                    })?;
                }
            }
        }

        self.closed = true;
        Ok(())
    }

    async fn sync_impl(&mut self, server_url: &str, auth_token: Option<String>) -> Result<()> {
        let database = Arc::clone(self.ensure_open()?);
        // Unauthenticated, the push registers ownership from the block
        // signatures but can grant nothing: `add_actor_relationship` needs a
        // caller to attribute the grant to.
        let auth_token = match (auth_token, self.identity.as_ref()) {
            (Some(token), _) => Some(token),
            (None, Some(identity)) => Some(identity.auth_token(audience_of(server_url))?),
            (None, None) => None,
        };
        // A bearer token on a cleartext origin is a credential anyone on the
        // path can take and replay. `localhost` is exempt because a browser
        // treats it as a secure context and it is where a node is developed
        // against.
        if auth_token.is_some() && !is_secure_origin(server_url) {
            return Err(WasmError::Sync(format!(
                "refusing to send a token to {server_url}: sync with a token needs https"
            )));
        }
        self.stop_sync().await;
        self.sync_task = Some(
            crate::sync::start(
                database,
                &self.event_bus,
                server_url,
                auth_token,
                self.grants(),
            )
            .await?,
        );
        Ok(())
    }

    fn grants(&self) -> crate::sync::Grants {
        crate::sync::Grants {
            relationships: self.grants.clone(),
            signer_identity: self
                .identity
                .as_ref()
                .map(ClientIdentity::signer_identity)
                .unwrap_or_default(),
        }
    }

    async fn stop_sync(&mut self) {
        if let Some(task) = self.sync_task.take() {
            task.stop().await;
        }
    }
}

/// The host a server URL points at, which is the audience its node checks a
/// token against.
///
/// The browser's parser is the right authority here: the node compares the
/// audience with the `Host` header the browser sends, and that header is this
/// same `host` — userinfo stripped, scheme lower-cased, a default port left
/// off, an IPv6 authority bracketed.
fn audience_of(server_url: &str) -> Option<String> {
    let host = web_sys::Url::new(server_url).ok()?.host();
    (!host.is_empty()).then_some(host)
}

/// Whether a browser treats this origin as secure, so a token may be sent to
/// it. `localhost` counts, as it does for every other browser API.
fn is_secure_origin(server_url: &str) -> bool {
    let Ok(url) = web_sys::Url::new(server_url) else {
        return false;
    };
    let hostname = url.hostname();
    url.protocol() == "https:"
        || hostname == "localhost"
        || hostname.ends_with(".localhost")
        || hostname == "127.0.0.1"
        || hostname == "[::1]"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::keys::{Key as _, PrivateKey as _};
    use identity::Identity as _;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    /// Create a client with a unique db name to isolate tests from OPFS state.
    fn test_config(name: &str) -> JsValue {
        serde_wasm_bindgen::to_value(&ClientConfig {
            db_name: Some(name.to_string()),
            ..Default::default()
        })
        .unwrap()
    }

    /// A client that authors with a key of its own, and that key's hex.
    async fn client_with_identity(name: &str) -> (DefraClient, String) {
        let private_key = crypto::generate_ed25519().unwrap();
        let private_key_hex = private_key.to_hex_string();
        let config = serde_wasm_bindgen::to_value(&ClientConfig {
            db_name: Some(name.to_string()),
            private_key: Some(private_key_hex.clone()),
            key_type: Some("ed25519".into()),
        })
        .unwrap();
        let client = DefraClient::create(config).await.unwrap();
        (client, private_key_hex)
    }

    /// Every signature over a block of this client's one document, as the
    /// signer identity each carries: Go's `PublicKey().String()`, so the
    /// hex-encoded public key as bytes.
    async fn signing_keys(client: &DefraClient) -> Vec<Vec<u8>> {
        signing_keys_by_document(client).await.remove(0)
    }

    /// The same, for every document this client holds.
    async fn signing_keys_by_document(client: &DefraClient) -> Vec<Vec<Vec<u8>>> {
        let engine = db::merge::BrowserSyncEngine::new(Arc::clone(client.db.as_ref().unwrap()));
        let mut documents = Vec::new();
        for document_ref in engine.document_refs().await.unwrap() {
            let document = engine
                .load_document(&document_ref)
                .await
                .unwrap()
                .expect("the document must be loadable");
            documents.push(signatures_in(&document));
        }
        documents
    }

    fn signatures_in(document: &defra_core::browser_sync::BrowserSyncDocument) -> Vec<Vec<u8>> {
        let blocks: std::collections::HashMap<String, Vec<u8>> = document
            .blocks
            .iter()
            .map(|block| (block.cid.clone(), hex::decode(&block.data).unwrap()))
            .collect();
        let mut keys = Vec::new();
        for bytes in blocks.values() {
            let Ok(block) = defra_core::block::Block::from_dag_cbor(bytes) else {
                continue;
            };
            let Some(signature_cid) = block.signature else {
                continue;
            };
            let signature =
                defra_core::block::Signature::from_dag_cbor(&blocks[&signature_cid.to_string()])
                    .expect("a block's signature must decode");
            keys.push(signature.header.identity);
        }
        keys
    }

    async fn create_one_document(client: &mut DefraClient) {
        client
            .add_schema("type User { name: String }")
            .await
            .unwrap();
        client
            .mutate(r#"mutation { create_User(input: {name: "Alice"}) { _docID } }"#)
            .await
            .unwrap();
    }

    #[wasm_bindgen_test]
    async fn a_client_signs_what_it_authors_with_its_own_key() {
        let (mut client, private_key_hex) = client_with_identity("signing_authored").await;
        let public_key_hex =
            crypto::private_key_from_string(crypto::KeyType::Ed25519, &private_key_hex)
                .unwrap()
                .public_key()
                .to_hex_string()
                .into_bytes();

        create_one_document(&mut client).await;

        let keys = signing_keys(&client).await;
        assert!(!keys.is_empty(), "the document must carry a signature");
        assert!(
            keys.iter().all(|key| *key == public_key_hex),
            "every signature must be made with this client's key"
        );
        client.close().await.unwrap();
    }

    /// The node signs for a caller whose key it holds; a client that holds its
    /// own signs for itself, and `did()` names who that is.
    #[wasm_bindgen_test]
    async fn a_client_reports_the_did_it_authors_as() {
        let (mut client, _) = client_with_identity("signing_did").await;
        let did = client.did().expect("a client with a key has a DID");
        assert!(did.starts_with("did:key:"), "unexpected DID: {did}");
        client.close().await.unwrap();
    }

    #[wasm_bindgen_test]
    async fn without_a_key_a_client_authors_unsigned_blocks() {
        let mut client = DefraClient::create(test_config("signing_none"))
            .await
            .unwrap();
        assert!(client.did().is_none());

        create_one_document(&mut client).await;

        assert!(
            signing_keys(&client).await.is_empty(),
            "a client with no key must not sign"
        );
        client.close().await.unwrap();
    }

    /// The token has to name the same DID the blocks are signed with, or the
    /// node registers a document to one identity and refuses grants from the
    /// other.
    #[wasm_bindgen_test]
    async fn a_minted_token_names_the_same_did_the_blocks_carry() {
        let (mut client, _) = client_with_identity("signing_token").await;
        let token = client.auth_token(Some("example.test:9181".into())).unwrap();

        let parsed = identity::from_token(token.as_bytes()).unwrap();
        identity::verify_auth_token(&parsed, "example.test:9181").unwrap();
        assert_eq!(
            parsed.did().unwrap().to_string(),
            client.did().unwrap(),
            "the token and the blocks must name one identity"
        );
        client.close().await.unwrap();
    }

    #[wasm_bindgen_test]
    async fn a_client_without_a_key_cannot_mint_a_token() {
        let client = DefraClient::create(test_config("signing_no_token"))
            .await
            .unwrap();
        assert!(client.auth_token(None).is_err());
    }

    /// `mutate` takes `&self`, so a page can start a second one before the
    /// first resolves, and the signing config they share is a thread-local.
    /// The mutation lock is what keeps one from taking it away from the other;
    /// this pins the outcome, since each mutation runs to completion without
    /// yielding here and the interleaving cannot be provoked from a test.
    #[wasm_bindgen_test]
    async fn concurrent_mutations_are_each_signed() {
        let (mut client, _) = client_with_identity("signing_concurrent").await;
        client
            .add_schema("type User { name: String }")
            .await
            .unwrap();

        let (first, second) = futures::join!(
            client.mutate(r#"mutation { create_User(input: {name: "Alice"}) { _docID } }"#),
            client.mutate(r#"mutation { create_User(input: {name: "Bob"}) { _docID } }"#),
        );
        first.unwrap();
        second.unwrap();

        let by_document = signing_keys_by_document(&client).await;
        assert_eq!(by_document.len(), 2, "both documents must exist");
        assert!(
            by_document.iter().all(|keys| !keys.is_empty()),
            "an interleaved mutation must not lose its signature"
        );
        client.close().await.unwrap();
    }

    /// A grant is applied as the caller and refused on a document that caller
    /// does not own, so attaching one to a relayed document would fail the push
    /// that carries it.
    #[wasm_bindgen_test]
    async fn grants_ride_only_on_documents_this_client_signed() {
        let (mut client, _) = client_with_identity("signing_grants").await;
        client
            .set_grants(
                serde_wasm_bindgen::to_value(&vec![BrowserSyncRelationship {
                    relation: "reader".into(),
                    target: "*".into(),
                }])
                .unwrap(),
            )
            .unwrap();
        create_one_document(&mut client).await;

        let engine = db::merge::BrowserSyncEngine::new(Arc::clone(client.db.as_ref().unwrap()));
        let refs = engine.document_refs().await.unwrap();
        let mut document = engine.load_document(&refs[0]).await.unwrap().unwrap();

        client.grants().attach(&mut document);
        assert_eq!(
            document.relationships.len(),
            1,
            "a document this client signed carries its grants"
        );

        // The same document as far as a relay is concerned: signed by somebody
        // whose key this client does not hold.
        let mut relayed = document.clone();
        relayed.relationships = Vec::new();
        crate::sync::Grants {
            relationships: vec![BrowserSyncRelationship {
                relation: "reader".into(),
                target: "*".into(),
            }],
            signer_identity: b"another-key".to_vec(),
        }
        .attach(&mut relayed);
        assert!(
            relayed.relationships.is_empty(),
            "a document signed by another key must go out untouched"
        );
        client.close().await.unwrap();
    }

    /// A browser generating its key with WebCrypto exports a JWK whose `d` is
    /// the 32-byte seed, so that is what a caller has in hand. It names the
    /// same identity as the 64-byte form this codebase stores.
    #[wasm_bindgen_test]
    async fn an_ed25519_seed_names_the_same_identity_as_the_full_key() {
        let private_key = crypto::generate_ed25519().unwrap();
        let full = private_key.raw().to_vec();
        assert_eq!(full.len(), 64, "the stored form is seed || public key");
        let seed = &full[..32];

        let mut client = DefraClient::create(test_config("signing_seed"))
            .await
            .unwrap();
        let from_seed = client.set_identity(&hex::encode(seed), "ed25519").unwrap();
        let from_full = client.set_identity(&hex::encode(&full), "ed25519").unwrap();
        assert_eq!(from_seed, from_full);

        // And it signs: the seed is a whole key here, not just an identifier.
        client
            .add_schema("type User { name: String }")
            .await
            .unwrap();
        client.set_identity(&hex::encode(seed), "ed25519").unwrap();
        client
            .mutate(r#"mutation { create_User(input: {name: "Alice"}) { _docID } }"#)
            .await
            .unwrap();
        let expected = private_key.public_key().to_hex_string().into_bytes();
        let keys = signing_keys(&client).await;
        assert!(!keys.is_empty() && keys.iter().all(|key| *key == expected));
        client.close().await.unwrap();
    }

    #[wasm_bindgen_test]
    async fn a_key_of_the_wrong_length_is_still_refused() {
        let mut client = DefraClient::create(test_config("signing_short"))
            .await
            .unwrap();
        assert!(client
            .set_identity(&hex::encode([7u8; 16]), "ed25519")
            .is_err());
        client.close().await.unwrap();
    }

    /// Better to refuse the key than to mint a token with it and fail every
    /// write: block signing needs a remote signer for secp256r1.
    #[wasm_bindgen_test]
    async fn a_key_that_cannot_sign_in_a_browser_is_refused() {
        let mut client = DefraClient::create(test_config("signing_r1"))
            .await
            .unwrap();
        let private_key = crypto::generate_secp256r1().unwrap();
        assert!(client
            .set_identity(&private_key.to_hex_string(), "secp256r1")
            .is_err());
        client.close().await.unwrap();
    }

    /// The node compares the audience with the `Host` header the browser sent,
    /// so the two have to be derived the same way.
    #[wasm_bindgen_test]
    fn an_audience_is_the_host_the_token_is_sent_to() {
        assert_eq!(
            audience_of("http://localhost:9181/api/v0"),
            Some("localhost:9181".into())
        );
        assert_eq!(
            audience_of("https://node.example"),
            Some("node.example".into())
        );
        // A browser leaves a default port off the Host header.
        assert_eq!(
            audience_of("https://node.example:443"),
            Some("node.example".into())
        );
        assert_eq!(
            audience_of("http://node.example:80/x"),
            Some("node.example".into())
        );
        assert_eq!(
            audience_of("HTTPS://Node.Example/x"),
            Some("node.example".into())
        );
        assert_eq!(
            audience_of("https://user:pass@node.example"),
            Some("node.example".into())
        );
        assert_eq!(audience_of("https://[::1]:9181"), Some("[::1]:9181".into()));
        assert_eq!(audience_of(""), None);
    }

    /// A bearer token on a cleartext origin is a credential anyone on the path
    /// can take, so sync refuses to send one.
    #[wasm_bindgen_test]
    async fn a_token_is_not_sent_over_cleartext() {
        let (mut client, _) = client_with_identity("signing_cleartext").await;
        let error = client
            .sync("http://node.example:9181", None)
            .await
            .expect_err("a token must not travel in the clear");
        assert!(
            format!("{error:?}").contains("https"),
            "unexpected error: {error:?}"
        );
        client.close().await.unwrap();
    }

    #[wasm_bindgen_test]
    fn localhost_is_a_secure_origin_for_a_token() {
        assert!(is_secure_origin("http://localhost:9181"));
        assert!(is_secure_origin("http://127.0.0.1:9181"));
        assert!(is_secure_origin("https://node.example"));
        assert!(!is_secure_origin("http://node.example"));
        assert!(!is_secure_origin("not a url"));
    }

    /// A closed client holds no database, so changing what it would author or
    /// minting a credential for it is a mistake worth reporting.
    #[wasm_bindgen_test]
    async fn a_closed_client_refuses_identity_and_token_operations() {
        let (mut client, private_key_hex) = client_with_identity("signing_closed").await;
        client.close().await.unwrap();

        assert!(client.set_identity(&private_key_hex, "ed25519").is_err());
        assert!(client.set_grants(JsValue::NULL).is_err());
        assert!(client.auth_token(None).is_err());
        // Reading back who it was is still fine.
        assert!(client.did().is_some());
    }

    #[wasm_bindgen_test]
    async fn test_client_creation() {
        let client = DefraClient::create(test_config("test_creation"))
            .await
            .unwrap();
        assert!(!client.closed);
    }

    #[wasm_bindgen_test]
    async fn test_close_is_idempotent() {
        let mut client = DefraClient::create(test_config("test_close_idem"))
            .await
            .unwrap();
        client.close().await.unwrap();
        assert!(client.closed);
        client.close().await.unwrap();
    }

    #[wasm_bindgen_test]
    async fn test_query_after_close_fails() {
        let mut client = DefraClient::create(test_config("test_query_closed"))
            .await
            .unwrap();
        client.close().await.unwrap();
        let result = client.query("{ User { name } }").await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_persist_after_close_fails() {
        let mut client = DefraClient::create(test_config("test_persist_closed"))
            .await
            .unwrap();
        client.close().await.unwrap();
        let result = client.persist().await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_mutate_no_schema_fails() {
        let client = DefraClient::create(test_config("test_mutate_no_schema"))
            .await
            .unwrap();
        let result = client
            .mutate(r#"mutation { add_User(input: {name: "Alice"}) { _docID } }"#)
            .await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_empty_mutation_fails() {
        let client = DefraClient::create(test_config("test_empty_mut"))
            .await
            .unwrap();
        let result = client.mutate("").await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_mutate_after_close_fails() {
        let mut client = DefraClient::create(test_config("test_mut_closed"))
            .await
            .unwrap();
        client.close().await.unwrap();
        let result = client
            .mutate(r#"mutation { add_User(input: {name: "Alice"}) { _docID } }"#)
            .await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_empty_query_fails() {
        let client = DefraClient::create(test_config("test_empty_q"))
            .await
            .unwrap();
        let result = client.query("").await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_empty_query_whitespace_fails() {
        let client = DefraClient::create(test_config("test_ws_q")).await.unwrap();
        let result = client.query("   ").await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_fresh_db_has_no_user_collections() {
        let client = DefraClient::create(test_config("test_fresh_db"))
            .await
            .unwrap();
        let result = client.get_collections().unwrap();
        let collections: Vec<CollectionInfo> = serde_wasm_bindgen::from_value(result).unwrap();
        // A fresh DB should have no user-defined collections (may have system ones)
        let user_types: Vec<_> = collections
            .iter()
            .filter(|c| !c.name.starts_with('_'))
            .collect();
        assert!(
            user_types.is_empty(),
            "Fresh DB should have no user collections, found: {:?}",
            user_types.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
    }

    #[wasm_bindgen_test]
    async fn test_add_schema_and_get_collections() {
        let mut client = DefraClient::create(test_config("test_add_schema"))
            .await
            .unwrap();

        let sdl = "type User { name: String, email: String }";
        client.add_schema(sdl).await.unwrap();

        let result = client.get_collections().unwrap();
        let collections: Vec<CollectionInfo> = serde_wasm_bindgen::from_value(result).unwrap();
        let user_col = collections.iter().find(|c| c.name == "User");
        assert!(user_col.is_some(), "User collection should exist");
        assert!(user_col.unwrap().fields.len() >= 2);
    }

    #[wasm_bindgen_test]
    async fn test_add_schema_invalid_sdl_fails() {
        let mut client = DefraClient::create(test_config("test_bad_sdl"))
            .await
            .unwrap();
        let result = client.add_schema("not valid graphql {{{{").await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_add_schema_rejects_invalid_self_relation_primaries() {
        let mut client = DefraClient::create(test_config("test_invalid_self_relations"))
            .await
            .unwrap();

        for sdl in [
            r#"
                type ZeroPrimary {
                    boss: ZeroPrimary @relation(name: "boss_minion")
                    minion: ZeroPrimary @relation(name: "boss_minion")
                }
            "#,
            r#"
                type BothPrimary {
                    boss: BothPrimary @primary @relation(name: "boss_minion")
                    minion: BothPrimary @primary @relation(name: "boss_minion")
                }
            "#,
        ] {
            let error = client.add_schema(sdl).await.unwrap_err();
            assert!(error
                .as_string()
                .unwrap()
                .contains("relation name is not unique within collection"));
        }

        let collections: Vec<CollectionInfo> =
            serde_wasm_bindgen::from_value(client.get_collections().unwrap()).unwrap();
        assert!(
            collections
                .iter()
                .all(|collection| collection.name != "ZeroPrimary"
                    && collection.name != "BothPrimary")
        );
    }

    #[wasm_bindgen_test]
    async fn test_add_duplicate_schema_fails() {
        let mut client = DefraClient::create(test_config("test_dup_schema"))
            .await
            .unwrap();

        let sdl = "type Item { name: String }";
        client.add_schema(sdl).await.unwrap();
        let result = client.add_schema(sdl).await;
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    async fn test_add_multiple_schemas() {
        let mut client = DefraClient::create(test_config("test_multi_schema"))
            .await
            .unwrap();

        client
            .add_schema("type Book { title: String }")
            .await
            .unwrap();
        client
            .add_schema("type Author { name: String }")
            .await
            .unwrap();

        let result = client.get_collections().unwrap();
        let collections: Vec<CollectionInfo> = serde_wasm_bindgen::from_value(result).unwrap();
        let names: Vec<&str> = collections.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Book"), "Book collection should exist");
        assert!(names.contains(&"Author"), "Author collection should exist");
    }

    #[wasm_bindgen_test]
    async fn test_query_empty_collection() {
        let mut client = DefraClient::create(test_config("test_query_empty"))
            .await
            .unwrap();

        client
            .add_schema("type Product { name: String, price: Int }")
            .await
            .unwrap();

        let result = client.query("{ Product { name price } }").await.unwrap();
        let response: serde_json::Value = serde_wasm_bindgen::from_value(result).unwrap();
        assert!(response.get("data").is_some());
    }

    #[wasm_bindgen_test]
    async fn test_persist_succeeds() {
        let mut client = DefraClient::create(test_config("test_persist_ok"))
            .await
            .unwrap();

        client
            .add_schema("type Note { text: String }")
            .await
            .unwrap();

        client.persist().await.unwrap();
    }

    // --- Mutation integration tests ---

    #[wasm_bindgen_test]
    async fn test_create_and_query_document() {
        let mut client = DefraClient::create(test_config("test_create_query"))
            .await
            .unwrap();
        client
            .add_schema("type User { name: String, age: Int }")
            .await
            .unwrap();

        let result = client
            .mutate(r#"mutation { add_User(input: {name: "Alice", age: 30}) { _docID name age } }"#)
            .await
            .unwrap();
        let response: serde_json::Value = serde_wasm_bindgen::from_value(result).unwrap();
        assert!(
            response.get("data").is_some(),
            "Mutation should return data"
        );

        let result = client.query("{ User { name age } }").await.unwrap();
        let response: serde_json::Value = serde_wasm_bindgen::from_value(result).unwrap();
        let data_str = response["data"].to_string();
        assert!(
            data_str.contains("Alice"),
            "Query should find Alice, got: {}",
            data_str
        );
        assert!(
            data_str.contains("30"),
            "Query should find age 30, got: {}",
            data_str
        );
    }

    #[wasm_bindgen_test]
    async fn test_create_multiple_documents() {
        let mut client = DefraClient::create(test_config("test_create_multi"))
            .await
            .unwrap();
        client
            .add_schema("type Person { name: String }")
            .await
            .unwrap();

        client
            .mutate(r#"mutation { add_Person(input: {name: "Bob"}) { _docID } }"#)
            .await
            .unwrap();
        client
            .mutate(r#"mutation { add_Person(input: {name: "Carol"}) { _docID } }"#)
            .await
            .unwrap();

        let result = client.query("{ Person { name } }").await.unwrap();
        let response: serde_json::Value = serde_wasm_bindgen::from_value(result).unwrap();
        let data_str = response["data"].to_string();
        assert!(
            data_str.contains("Bob"),
            "Should find Bob, got: {}",
            data_str
        );
        assert!(
            data_str.contains("Carol"),
            "Should find Carol, got: {}",
            data_str
        );
    }

    #[wasm_bindgen_test]
    async fn test_create_returns_doc_id() {
        let mut client = DefraClient::create(test_config("test_create_docid"))
            .await
            .unwrap();
        client
            .add_schema("type Widget { label: String }")
            .await
            .unwrap();

        let result = client
            .mutate(r#"mutation { add_Widget(input: {label: "test"}) { _docID } }"#)
            .await
            .unwrap();
        let response: serde_json::Value = serde_wasm_bindgen::from_value(result).unwrap();
        let data_str = response["data"].to_string();
        assert!(
            data_str.contains("_docID"),
            "Mutation result should include _docID, got: {}",
            data_str
        );
    }

    #[wasm_bindgen_test]
    async fn test_create_persist_reopen_query() {
        // Create a doc, persist, close, reopen, and verify data survives
        let db_name = "test_create_persist_reopen";

        {
            let mut client = DefraClient::create(test_config(db_name)).await.unwrap();
            client
                .add_schema("type Task { title: String }")
                .await
                .unwrap();
            client
                .mutate(r#"mutation { add_Task(input: {title: "Survive"}) { _docID } }"#)
                .await
                .unwrap();
            // mutate_impl auto-persists, but explicit persist for clarity
            client.persist().await.unwrap();
            client.close().await.unwrap();
        }

        // Reopen the same database
        let client = DefraClient::create(test_config(db_name)).await.unwrap();

        let result = client.query("{ Task { title } }").await.unwrap();
        let response: serde_json::Value = serde_wasm_bindgen::from_value(result).unwrap();
        let data_str = response["data"].to_string();
        assert!(
            data_str.contains("Survive"),
            "Data should survive persist→close→reopen, got: {}",
            data_str
        );
    }
}
