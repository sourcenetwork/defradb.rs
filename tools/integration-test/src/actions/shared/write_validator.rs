//! The write validator a scenario starts a Rust node with: an embedder's
//! `query::access_hooks::WriteValidator` (`defra.write_validator`).

use async_trait::async_trait;
use query::access_hooks::{WriteRequest, WriteValidator};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Refuses, with `message`, every mutation whose add or update input sets
/// `field` to `value`; any other mutation runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefuseWrites {
    pub field: String,
    pub value: Value,
    pub message: String,
}

#[async_trait]
impl WriteValidator for RefuseWrites {
    async fn validate_write(&self, request: &WriteRequest<'_>) -> Result<(), String> {
        let refused = request
            .create_input
            .iter()
            .chain(std::iter::once(request.update_input))
            .any(|input| input.get(&self.field) == Some(&self.value));
        match refused {
            true => Err(self.message.clone()),
            false => Ok(()),
        }
    }
}
