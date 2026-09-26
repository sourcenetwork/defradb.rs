//! A browser peer dialled into the node through its relay.

use defra_wasm::DefraClient;
use serde_json::{json, Value};
use wasm_bindgen::JsValue;

use crate::key::Key;
use crate::node;

pub struct Browser {
    pub client: DefraClient,
    pub address: String,
    node_address: String,
}

impl Browser {
    /// Authors as `key` when given, and writes unsigned blocks otherwise.
    pub async fn start(db_name: &str, key: Option<&Key>, sdl: &str) -> Self {
        let config = match key {
            Some(key) => json!({
                "db_name": db_name,
                "private_key": key.private_key_hex,
                "key_type": "secp256k1",
            }),
            None => json!({ "db_name": db_name }),
        };
        Self::start_with(config, sdl).await
    }

    /// As [`Self::start`], from a whole `create` config.
    pub async fn start_with(config: Value, sdl: &str) -> Self {
        let mut client = DefraClient::create(js(&config)).await.unwrap();
        client.add_schema(sdl).await.unwrap();
        let endpoint_id = client
            .start_p2p(js(&json!({ "relay_urls": [node::RELAY] })))
            .await
            .unwrap();
        let node_address = node::address(&node::endpoint_id().await);
        client.connect_peer(&node_address).await.unwrap();
        Self {
            client,
            address: node::address(&endpoint_id),
            node_address,
        }
    }

    pub async fn replicate_to_node(&self, collections: &[&str]) {
        self.client
            .add_replicator(&self.node_address, js(&json!(collections)))
            .await
            .unwrap();
    }

    pub async fn query(&self, query: &str) -> Value {
        let result: Value =
            serde_wasm_bindgen::from_value(self.client.query(query).await.unwrap()).unwrap();
        result["data"].clone()
    }

    pub async fn mutate(&self, mutation: &str) -> Value {
        let result: Value =
            serde_wasm_bindgen::from_value(self.client.mutate(mutation).await.unwrap()).unwrap();
        result["data"].clone()
    }

    pub async fn governance(&self) -> Value {
        serde_wasm_bindgen::from_value(self.client.governance().await.unwrap()).unwrap()
    }
}

/// A plain JS object. `serde_wasm_bindgen` turns a JSON map into a JS `Map`,
/// whose entries a struct with defaulted fields silently ignores.
fn js(value: &Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap()
}
