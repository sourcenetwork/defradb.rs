//! DefraClient - the main WASM client interface.
//!
//! Provides a high-level API for browser applications to interact with DefraDB.
//! This wraps the `db` crate's `DB` type with a JavaScript-friendly interface.

use std::sync::Arc;

use wasm_bindgen::prelude::*;

use db::{
    AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB,
};
use events::Bus;
use query::runner::QueryRunner;
use storage::RegolithStore;

type WasmRunner =
    QueryRunner<LensedAutoCommitFetcher<RegolithStore>, DbTransactionRegistry<RegolithStore>>;

use crate::bindings::{from_js, to_js, ClientConfig, CollectionInfo, FieldInfo};
use crate::document_changes::DocumentChanges;
use crate::error::{Result, WasmError};
use crate::governance::Governance;
use crate::identity::{ClientIdentity, SigningGuard};
use crate::p2p::P2PRuntime;

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
    pub(crate) event_bus: Arc<events::ChannelBus>,
    pub(crate) document_acp: Arc<dyn acp::DocumentACP>,
    pub(crate) p2p: Option<P2PRuntime>,
    pub(crate) identity: Option<ClientIdentity>,
    pub(crate) governance: Option<Governance>,
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
    /// - `db_name`: the OPFS directory the store lives in (optional)
    /// - `private_key`, `key_type`: a key to author with (optional)
    /// - `require_sync_handles`: fail instead of falling back to the
    ///   in-memory mirror when OPFS synchronous access handles are refused,
    ///   so a Worker taking over a database can retry (optional)
    /// - `governance`: `{ collections, rule_modules, budget, engine }`, the
    ///   collections the application claims, judged by the wasm rule module
    ///   each version names (optional; see [`crate::governance::GovernanceConfig`])
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
        // A running peer keeps proving the identity it started with, so a new
        // one would sign writes as a DID its peers cannot tie to this endpoint.
        if self.p2p.is_some() {
            return Err(WasmError::P2P("stop P2P before changing identity".into()).into());
        }
        let identity = ClientIdentity::from_private_key(private_key_hex, key_type)?;
        let did = identity.did().to_string();
        self.identity = Some(identity);
        Ok(did)
    }

    /// The DID this client authors as, or `undefined` when it holds no key.
    #[wasm_bindgen]
    pub fn did(&self) -> Option<String> {
        self.identity.as_ref().map(|id| id.did().to_string())
    }

    /// Notifications of documents changing in this client, by a local write
    /// or a merge from a peer. See [`DocumentChanges`].
    #[wasm_bindgen]
    pub fn document_changes(&self) -> std::result::Result<DocumentChanges, JsValue> {
        self.ensure_open()?;
        Ok(DocumentChanges::new(
            self.event_bus.subscribe_document_changes(),
        ))
    }

    /// A JWT proving possession of this client's key, for `audience` — the host
    /// of the node's HTTP API it will be sent to, which is what that node checks
    /// it against.
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
        let store = RegolithStore::open_opfs_with(db_name, config.require_sync_handles)
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
        // Before the runner exists: nothing can write or merge ungoverned.
        let governance = match config.governance.as_ref() {
            Some(governance) => Some(Governance::install(&db, governance).await?),
            None => None,
        };
        let document_acp: Arc<dyn acp::DocumentACP> =
            Arc::new(acp::ZanzibarDocumentACP::new(Arc::new(
                acp::PersistentZanzibarStore::from_store(Arc::clone(db.store())),
            )));
        let runner = build_runner(&db, &document_acp, None);

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
            document_acp,
            p2p: None,
            identity,
            governance,
            mutate_lock: futures::lock::Mutex::new(()),
            closed: false,
        })
    }

    /// Swap in a runner whose writes go through the P2P mutator while a peer
    /// is running, and through plain auto-commit otherwise.
    pub(crate) fn rebuild_runner(&mut self) {
        if let Some(db) = self.db.as_ref() {
            self.runner = Some(build_runner(db, &self.document_acp, self.p2p.as_ref()));
        }
    }

    /// Who document ACP sees as the caller: the identity this client authors as.
    fn caller(&self) -> Option<identity::Did> {
        self.identity
            .as_ref()
            .and_then(|identity| identity::Did::new(identity.did()).ok())
    }

    pub(crate) fn ensure_open(&self) -> Result<&Arc<DB<RegolithStore>>> {
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

        match runner
            .execute_query_with_identity(graphql, self.caller())
            .await
        {
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
            .execute_mutation_with_identity(graphql, self.caller())
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

        self.stop_p2p_impl().await;
        if let Some(governance) = self.governance.as_mut() {
            governance.stop().await;
        }
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
}

// Nothing here is Send on wasm32, and nothing needs to be.
#[allow(clippy::arc_with_non_send_sync)]
fn build_runner(
    db: &Arc<DB<RegolithStore>>,
    document_acp: &Arc<dyn acp::DocumentACP>,
    p2p: Option<&P2PRuntime>,
) -> WasmRunner {
    let (mutator, registry): (Arc<dyn query::DocMutator>, _) = match p2p {
        Some(runtime) => (
            Arc::clone(&runtime.mutator),
            DbTransactionRegistry::with_broadcaster(
                Arc::clone(db),
                Arc::clone(&runtime.txn_broadcaster),
            ),
        ),
        None => (
            Arc::new(AutoCommitMutator::new(Arc::clone(db))),
            DbTransactionRegistry::new(Arc::clone(db)),
        ),
    };
    QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(db)),
        DbCollectionProvider::new_arc(Arc::clone(db)),
        Arc::new(registry),
    )
    .with_mutator(mutator)
    .with_collection_truncator(db::DbCollectionTruncator::new_arc(Arc::clone(db)))
    .with_acp(Arc::clone(document_acp))
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
            ..Default::default()
        })
        .unwrap();
        let client = DefraClient::create(config).await.unwrap();
        (client, private_key_hex)
    }

    /// The signer identity of every signed block this client holds, grouped
    /// by document: Go's `PublicKey().String()`, the hex-encoded public key.
    async fn signing_keys_by_document(client: &DefraClient) -> Vec<Vec<String>> {
        let result = client
            .query("query { _commits { docID signature { identity } } }")
            .await
            .unwrap();
        let result: serde_json::Value = serde_wasm_bindgen::from_value(result).unwrap();
        let mut by_document: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for commit in result["data"]["_commits"].as_array().unwrap() {
            let keys = by_document
                .entry(commit["docID"].as_str().unwrap().to_string())
                .or_default();
            if let Some(identity) = commit["signature"]["identity"].as_str() {
                keys.push(identity.to_string());
            }
        }
        by_document.into_values().collect()
    }

    /// The same, for a client holding exactly one document.
    async fn signing_keys(client: &DefraClient) -> Vec<String> {
        signing_keys_by_document(client).await.remove(0)
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
                .to_hex_string();

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

    /// The token has to name the same DID the blocks are signed with, or a
    /// node's HTTP API would see a different caller than the one that authored.
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
        let expected = private_key.public_key().to_hex_string();
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

    /// A closed client holds no database, so changing what it would author or
    /// minting a credential for it is a mistake worth reporting.
    #[wasm_bindgen_test]
    async fn a_closed_client_refuses_identity_and_token_operations() {
        let (mut client, private_key_hex) = client_with_identity("signing_closed").await;
        client.close().await.unwrap();

        assert!(client.set_identity(&private_key_hex, "ed25519").is_err());
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

    /// Tests run on the page's main thread, where browsers refuse synchronous
    /// access handles. Requiring them must fail the open rather than mount
    /// the mirror, and must leave nothing held that stops the same database
    /// opening without the requirement.
    #[wasm_bindgen_test]
    async fn a_client_that_requires_sync_handles_is_refused_off_a_worker() {
        let required = serde_wasm_bindgen::to_value(&ClientConfig {
            db_name: Some("test_require_sync_handles".to_string()),
            require_sync_handles: true,
            ..Default::default()
        })
        .unwrap();
        let refused = match DefraClient::create(required).await {
            Ok(_) => panic!("a client requiring sync handles opened off a worker"),
            Err(error) => error.as_string().unwrap_or_default(),
        };
        assert!(
            refused.contains("sync access handles"),
            "the refusal should name the requirement: {refused}"
        );

        let client = DefraClient::create(test_config("test_require_sync_handles"))
            .await
            .unwrap();
        assert!(!client.closed);
    }

    /// A write names its document in the next batch, marked local, and closing
    /// the client ends the stream with null rather than leaving it pending.
    #[wasm_bindgen_test]
    async fn a_local_write_arrives_as_a_document_change() {
        let mut client = DefraClient::create(test_config("test_document_changes"))
            .await
            .unwrap();
        client
            .add_schema("type Note { text: String }")
            .await
            .unwrap();
        let mut changes = client.document_changes().unwrap();

        let created = client
            .mutate(r#"mutation { add_Note(input: {text: "hello"}) { _docID } }"#)
            .await
            .unwrap();
        let created: serde_json::Value = serde_wasm_bindgen::from_value(created).unwrap();
        let added = &created["data"]["add_Note"];
        let doc_id = added[0]["_docID"]
            .as_str()
            .or_else(|| added["_docID"].as_str())
            .unwrap_or_else(|| panic!("no _docID in {created}"))
            .to_string();

        let batch: serde_json::Value =
            serde_wasm_bindgen::from_value(changes.next_batch().await.unwrap()).unwrap();
        let change = batch["changes"]
            .as_array()
            .and_then(|all| {
                all.iter()
                    .find(|change| change["doc_id"] == doc_id.as_str())
            })
            .unwrap_or_else(|| panic!("{doc_id} is not in {batch}"));
        assert_eq!(change["local"], true, "a write here is local: {batch}");

        client.close().await.unwrap();
        assert!(
            changes.next_batch().await.unwrap().is_null(),
            "a closed client ends its change stream"
        );
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
