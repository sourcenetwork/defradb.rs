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
                "dbName": db_name,
                "privateKey": key.private_key_hex,
                "keyType": "secp256k1",
            }),
            None => json!({ "dbName": db_name }),
        };
        let mut client = DefraClient::create(js(&config)).await.unwrap();
        client.add_collection(sdl).await.unwrap();
        let endpoint_id = client
            .start_p2p(js(&json!({ "relayUrls": [node::RELAY] })))
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
        self.exec(query).await
    }

    pub async fn mutate(&self, mutation: &str) -> Value {
        self.exec(mutation).await
    }

    async fn exec(&self, request: &str) -> Value {
        let result: Value = serde_wasm_bindgen::from_value(
            self.client
                .exec_request(request, JsValue::UNDEFINED)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(
            result["gql"]["errors"].is_null(),
            "request failed: {request}: {}",
            result["gql"]["errors"]
        );
        result["gql"]["data"].clone()
    }
}

/// A plain JS object. `serde_wasm_bindgen` turns a JSON map into a JS `Map`,
/// whose entries a struct with defaulted fields silently ignores.
fn js(value: &Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap()
}
