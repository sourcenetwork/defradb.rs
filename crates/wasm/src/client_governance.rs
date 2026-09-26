//! The governance half of `DefraClient`: the rule modules it holds and
//! what it reports about the collections it claims.

use wasm_bindgen::prelude::*;

use crate::bindings::to_js;
use crate::client::DefraClient;
use crate::error::WasmError;

#[wasm_bindgen]
impl DefraClient {
    /// Hold a wasm rule module, returning the CID a version's rule tag
    /// names it by (CIDv1, raw codec, SHA2-256 of the bytes).
    ///
    /// Refused when the module does not compile on this client's engine,
    /// or when the client was created without `governance`. A composite
    /// deferred because its rule was not held is judged on the next sweep.
    ///
    /// ```javascript
    /// const cid = await client.put_rule_module(new Uint8Array(await (await fetch('/rules.wasm')).arrayBuffer()));
    /// ```
    #[wasm_bindgen]
    pub async fn put_rule_module(&mut self, bytes: &[u8]) -> std::result::Result<String, JsValue> {
        self.ensure_open()?;
        let governance = self.governance.as_mut().ok_or_else(not_governed)?;
        Ok(governance.put_module(bytes).await?.to_string())
    }

    /// What this client governs, or `null` when it was created without
    /// `governance`:
    ///
    /// ```text
    /// { collections: [name], engine: "wasmi",
    ///   budget: { fuel, steps, memory_bytes },
    ///   modules: [cid],       // held through this client since it opened
    ///   rules: [{ collection, version_id, rule, held }],
    ///   sweep: "local" | "peer" }
    /// ```
    ///
    /// `rules` lists each claimed collection this node holds, with the rule
    /// its version names and whether the module is held here. A rule not
    /// held defers every write judged under it.
    #[wasm_bindgen]
    pub async fn governance(&self) -> std::result::Result<JsValue, JsValue> {
        let db = self.ensure_open()?;
        match self.governance.as_ref() {
            None => Ok(JsValue::NULL),
            Some(governance) => Ok(to_js(&governance.status(db).await?)?),
        }
    }
}

fn not_governed() -> WasmError {
    WasmError::Governance("this client was created without governance".to_string())
}
