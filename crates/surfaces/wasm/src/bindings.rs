//! JavaScript interop helpers.
//!
//! Utilities for converting between Rust types and JavaScript values.

use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

use crate::error::{Result, WasmError};

/// Convert a Rust value to a JavaScript value.
pub fn to_js<T: Serialize>(value: &T) -> Result<JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(WasmError::from)
}

/// Convert a JavaScript value to a Rust type.
pub fn from_js<T: DeserializeOwned>(value: JsValue) -> Result<T> {
    serde_wasm_bindgen::from_value(value).map_err(WasmError::from)
}

/// Convert a JavaScript config object to a Rust type, honouring
/// `deny_unknown_fields`: `serde_wasm_bindgen` reads only a struct's declared
/// fields, so an unknown key would otherwise pass unseen.
pub fn config_from_js<T>(value: JsValue) -> Result<T>
where
    T: DeserializeOwned,
{
    let value: serde_json::Value = from_js(value)?;
    Ok(serde_json::from_value(value)?)
}

/// Client configuration passed from JavaScript.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientConfig {
    /// Database name for storage
    pub db_name: Option<String>,
    /// Hex-encoded private key this client authors with, if it holds one.
    pub private_key: Option<String>,
    /// Key type of `privateKey`: ed25519, secp256k1 or secp256r1.
    pub key_type: Option<String>,
    /// Fail to open unless OPFS synchronous access handles are granted,
    /// rather than falling back to the in-memory mirror. For a client in a
    /// dedicated Worker that may take over a database another Worker has
    /// not yet released: the open then fails and can be retried, instead of
    /// mounting an empty mirror beside the real data.
    pub require_sync_handles: bool,
}

/// Options for `execRequest`, the subset of Go's this client can honour.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ExecRequestOptions {
    /// Values for the request's `$variables`.
    pub variables: Option<rapidhash::RapidHashMap<String, serde_json::Value>>,
    /// The transaction to run it in, an ID from `txnBegin`.
    #[serde(rename = "txnId")]
    pub txn_id: Option<String>,
}

impl ExecRequestOptions {
    /// Read the options object, rejecting any key but `variables` and `txnId`: an option
    /// this client cannot honour must fail, not be ignored.
    pub fn from_js(opts: JsValue) -> Result<Self> {
        if opts.is_undefined() || opts.is_null() {
            return Ok(Self::default());
        }
        for key in js_sys::Object::keys(&js_sys::Object::from(opts.clone())) {
            if !matches!(key.as_string().as_deref(), Some("variables" | "txnId")) {
                return Err(WasmError::Query(format!(
                    "unsupported execRequest option: {}",
                    key.as_string().unwrap_or_default()
                )));
            }
        }
        from_js(opts)
    }
}
