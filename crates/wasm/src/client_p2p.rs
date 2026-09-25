//! The P2P half of `DefraClient`: running this browser as a peer.

use std::sync::Arc;

use defra_p2p_adapter::{P2PError, P2POperations, ReplicationFilters};
use wasm_bindgen::prelude::*;

use crate::bindings::{from_js, to_js};
use crate::client::DefraClient;
use crate::error::{Result, WasmError};
use crate::p2p::{P2PConfig, P2PRuntime};

#[wasm_bindgen]
impl DefraClient {
    /// Join the network as a peer, reaching others through a relay.
    ///
    /// `config` is `{ relay_urls, secret_key_hex, discovery }`, all optional.
    /// With an identity set and no `secret_key_hex`, the endpoint key is
    /// derived from the identity, so the endpoint id survives a reload.
    /// Returns the endpoint id, which is what a peer dials back.
    ///
    /// Writes made from here on are announced to peers.
    ///
    /// # Example
    ///
    /// ```javascript
    /// const id = await client.start_p2p({ relay_urls: ['https://relay.example.com'] });
    /// await client.connect_peer(`${nodeId}@https://relay.example.com`);
    /// await client.add_replicator(`${nodeId}@https://relay.example.com`, ['User']);
    /// ```
    #[wasm_bindgen]
    pub async fn start_p2p(&mut self, config: JsValue) -> std::result::Result<String, JsValue> {
        self.start_p2p_impl(config).await.map_err(Into::into)
    }

    /// Leave the network. Local reads and writes keep working.
    #[wasm_bindgen]
    pub async fn stop_p2p(&mut self) -> std::result::Result<(), JsValue> {
        self.ensure_open()?;
        self.stop_p2p_impl().await;
        Ok(())
    }

    /// `{ id, addresses }` for this peer.
    #[wasm_bindgen]
    pub async fn peer_info(&self) -> std::result::Result<JsValue, JsValue> {
        let ops = self.p2p_ops()?;
        let id = ops.local_peer_id().await.map_err(p2p_error)?;
        let addresses = ops.listen_addresses().await.map_err(p2p_error)?;
        Ok(to_js(
            &serde_json::json!({ "id": id, "addresses": addresses }),
        )?)
    }

    /// Dial a peer by ticket or `<endpoint-id>@<relay-url>`.
    #[wasm_bindgen]
    pub async fn connect_peer(&self, address: &str) -> std::result::Result<(), JsValue> {
        Ok(self
            .p2p_ops()?
            .connect_peer(address)
            .await
            .map_err(p2p_error)?)
    }

    /// Endpoint ids currently holding a live connection.
    #[wasm_bindgen]
    pub async fn connected_peers(&self) -> std::result::Result<JsValue, JsValue> {
        let peers = self.p2p_ops()?.connected_peers().await.map_err(p2p_error)?;
        Ok(to_js(&peers)?)
    }

    /// Push these collections to the peer at `address`, now and on every write.
    #[wasm_bindgen]
    pub async fn add_replicator(
        &self,
        address: &str,
        collections: JsValue,
    ) -> std::result::Result<(), JsValue> {
        let collections = collection_names(collections)?;
        Ok(self
            .p2p_ops()?
            .add_replicator(
                collections,
                Some(address),
                ReplicationFilters::new(),
                Vec::new(),
                None,
            )
            .await
            .map_err(p2p_error)?)
    }

    /// Stop pushing these collections to the peer at `address`.
    #[wasm_bindgen]
    pub async fn delete_replicator(
        &self,
        address: &str,
        collections: JsValue,
    ) -> std::result::Result<(), JsValue> {
        let collections = collection_names(collections)?;
        Ok(self
            .p2p_ops()?
            .remove_replicator(collections, Some(address))
            .await
            .map_err(p2p_error)?)
    }

    /// Every replicator this peer pushes to.
    #[wasm_bindgen]
    pub async fn replicators(&self) -> std::result::Result<JsValue, JsValue> {
        let replicators = self.p2p_ops()?.get_replicators().await.map_err(p2p_error)?;
        Ok(to_js(&replicators)?)
    }

    /// Follow writes other peers announce on these collections.
    #[wasm_bindgen]
    pub async fn subscribe_collections(
        &self,
        collections: JsValue,
    ) -> std::result::Result<(), JsValue> {
        let collections = collection_names(collections)?;
        Ok(self
            .p2p_ops()?
            .add_collections(collections)
            .await
            .map_err(p2p_error)?)
    }

    /// Stop following these collections.
    #[wasm_bindgen]
    pub async fn unsubscribe_collections(
        &self,
        collections: JsValue,
    ) -> std::result::Result<(), JsValue> {
        let collections = collection_names(collections)?;
        Ok(self
            .p2p_ops()?
            .remove_collections(collections)
            .await
            .map_err(p2p_error)?)
    }
}

impl DefraClient {
    async fn start_p2p_impl(&mut self, config: JsValue) -> Result<String> {
        let database = Arc::clone(self.ensure_open()?);
        if self.p2p.is_some() {
            return Err(WasmError::P2P("P2P is already running".into()));
        }
        let config: P2PConfig = if config.is_undefined() || config.is_null() {
            P2PConfig::default()
        } else {
            from_js(config)?
        };

        let runtime = P2PRuntime::start(
            database,
            self.event_bus.clone(),
            Arc::clone(&self.document_acp),
            self.identity.as_ref().map(|identity| identity.raw()),
            &config,
        )
        .await?;
        let endpoint_id = runtime.endpoint_id();
        self.p2p = Some(runtime);
        if let Some(governance) = self.governance.as_ref() {
            governance.set_peer_running(true);
        }
        self.rebuild_runner();
        Ok(endpoint_id)
    }

    pub(crate) async fn stop_p2p_impl(&mut self) {
        if let Some(runtime) = self.p2p.take() {
            self.rebuild_runner();
            runtime.shutdown().await;
            if let Some(governance) = self.governance.as_ref() {
                governance.set_peer_running(false);
            }
        }
    }

    fn p2p_ops(&self) -> Result<&Arc<dyn P2POperations>> {
        self.ensure_open()?;
        self.p2p
            .as_ref()
            .map(|runtime| &runtime.ops)
            .ok_or_else(|| WasmError::P2P("P2P is not running; call start_p2p first".into()))
    }
}

fn collection_names(collections: JsValue) -> Result<Vec<String>> {
    if collections.is_undefined() || collections.is_null() {
        return Ok(Vec::new());
    }
    from_js(collections)
}

fn p2p_error(error: P2PError) -> WasmError {
    WasmError::P2P(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::ClientConfig;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    async fn client(name: &str, private_key: Option<String>) -> DefraClient {
        let key_type = private_key.as_ref().map(|_| "ed25519".to_string());
        DefraClient::create(
            serde_wasm_bindgen::to_value(&ClientConfig {
                db_name: Some(name.to_string()),
                private_key,
                key_type,
                ..Default::default()
            })
            .unwrap(),
        )
        .await
        .unwrap()
    }

    fn no_relays() -> JsValue {
        js_sys::JSON::parse(r#"{"relay_urls":["http://127.0.0.1:9"]}"#).unwrap()
    }

    #[wasm_bindgen_test]
    async fn a_browser_binds_an_endpoint() {
        let mut client = client("p2p_bind", None).await;
        let id = client.start_p2p(no_relays()).await.unwrap();
        assert_eq!(id.len(), 64);

        let connected: Vec<String> =
            serde_wasm_bindgen::from_value(client.connected_peers().await.unwrap()).unwrap();
        assert!(connected.is_empty());
        client.close().await.unwrap();
    }

    /// A peer that stored this browser as a replicator dials the same id after
    /// a reload, which only works if the identity fixes the endpoint key.
    #[wasm_bindgen_test]
    async fn an_identity_keeps_the_endpoint_id_across_restarts() {
        let key = crypto::keys::Key::to_hex_string(&crypto::generate_ed25519().unwrap());
        let mut client = client("p2p_stable_id", Some(key)).await;

        let first = client.start_p2p(no_relays()).await.unwrap();
        client.stop_p2p().await.unwrap();
        let second = client.start_p2p(no_relays()).await.unwrap();

        assert_eq!(first, second);
        client.close().await.unwrap();
    }

    /// The peer proves the identity it started with, so the client may not
    /// author as another one until the peer is restarted under it.
    #[wasm_bindgen_test]
    async fn identity_is_fixed_while_p2p_runs() {
        let first_key = crypto::keys::Key::to_hex_string(&crypto::generate_ed25519().unwrap());
        let second_key = crypto::keys::Key::to_hex_string(&crypto::generate_ed25519().unwrap());
        let mut client = client("p2p_identity_fixed", Some(first_key)).await;
        let first_did = client.did().unwrap();

        let first_id = client.start_p2p(no_relays()).await.unwrap();
        assert!(client.set_identity(&second_key, "ed25519").is_err());
        assert_eq!(client.did().unwrap(), first_did);

        client.stop_p2p().await.unwrap();
        let second_did = client.set_identity(&second_key, "ed25519").unwrap();
        assert_ne!(second_did, first_did);
        let second_id = client.start_p2p(no_relays()).await.unwrap();
        assert_ne!(second_id, first_id);
        client.close().await.unwrap();
    }

    #[wasm_bindgen_test]
    async fn peer_operations_need_a_running_peer() {
        let mut client = client("p2p_not_started", None).await;
        assert!(client.peer_info().await.is_err());
        assert!(client.connect_peer("anything").await.is_err());
        client.close().await.unwrap();
    }

    #[wasm_bindgen_test]
    async fn starting_twice_is_refused() {
        let mut client = client("p2p_twice", None).await;
        client.start_p2p(no_relays()).await.unwrap();
        assert!(client.start_p2p(no_relays()).await.is_err());
        client.close().await.unwrap();
    }

    /// Stopping swaps the P2P mutator back out, so local writes must still land.
    #[wasm_bindgen_test]
    async fn writes_work_before_during_and_after_p2p() {
        let mut client = client("p2p_writes", None).await;
        client
            .add_schema("type Note { text: String }")
            .await
            .unwrap();
        let create = |text: &str| {
            format!(r#"mutation {{ create_Note(input: {{text: "{text}"}}) {{ _docID }} }}"#)
        };

        client.mutate(&create("before")).await.unwrap();
        client.start_p2p(no_relays()).await.unwrap();
        client.mutate(&create("during")).await.unwrap();
        client.stop_p2p().await.unwrap();
        client.mutate(&create("after")).await.unwrap();

        let result: serde_json::Value =
            serde_wasm_bindgen::from_value(client.query("{ Note { text } }").await.unwrap())
                .unwrap();
        assert_eq!(result["data"]["Note"].as_array().unwrap().len(), 3);
        client.close().await.unwrap();
    }
}
