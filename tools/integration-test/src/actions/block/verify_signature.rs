use crate::actions::shared::KeyType;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::Text;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Verifies a block's signature against the node that signed it, or the identity seed that did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyBlockSignature {
    #[serde(rename = "blockCID")]
    pub cid: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for VerifyBlockSignature {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        let identity = self.common.client_identity(Self::name())?;
        let (public_key, key_type) = self.signer(state)?;
        let key_type = match key_type {
            KeyType::Secp256k1 => crypto::KeyType::Secp256k1,
            KeyType::Ed25519 => crypto::KeyType::Ed25519,
        };
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .verify_block_signature_as(
                        &state.context_as(self.txn, identity),
                        self.cid.as_str(),
                        &public_key,
                        key_type,
                    ),
            )
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl VerifyBlockSignature {
    /// The public key (hex) and key type of the signer the action names: a
    /// node's signing key, or client identity `signer_seed`'s.
    fn signer(&self, state: &State) -> Result<(String, KeyType), Failure> {
        match (&self.signer_node, self.signer_seed) {
            (Some(node), _) => state.signer_of(node).ok_or_else(|| {
                Failure::operation(format!("node {node} was started with no signing key"))
            }),
            (None, Some(seed)) => Ok((
                hex::encode(crate::scenario::identity::identity(seed).public_key_bytes()),
                KeyType::Secp256k1,
            )),
            (None, None) => Err(Failure::operation(format!(
                "{} names no signer",
                Self::name()
            ))),
        }
    }

    /// Fails on a transaction: the HTTP API and the CLI verify outside one.
    fn unsupported_txn(&self) -> Result<(), Failure> {
        match self.txn {
            Some(_) => Err(Failure::unsupported(format!(
                "{} argument txn over HTTP or the CLI",
                Self::name()
            ))),
            None => Ok(()),
        }
    }
}

/// The key type as the HTTP API and the CLI name it.
fn key_type_name(key_type: KeyType) -> &'static str {
    match key_type {
        KeyType::Secp256k1 => "secp256k1",
        KeyType::Ed25519 => "ed25519",
    }
}

impl Action for VerifyBlockSignature {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for VerifyBlockSignature {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            action.unsupported_txn()?;
            let (public_key, key_type) = action.signer(state)?;
            let cid = action.cid.as_str().to_owned();
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client
                        .block_verify_signature(&cid, &public_key, Some(key_type_name(key_type)))
                        .await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for VerifyBlockSignature {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            action.unsupported_txn()?;
            let (public_key, key_type) = action.signer(state)?;
            let args = [
                "block",
                "verify-signature",
                "--key-type",
                key_type_name(key_type),
                public_key.as_str(),
                action.cid.as_str(),
            ];
            state
                .defra_client_as(action.common.node(), None, identity, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
