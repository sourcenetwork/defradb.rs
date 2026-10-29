use crate::actions::shared::{IndexGate, KeyType, NodeClient};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Starts a node. Without options it is a plain in-process node; every option below is one of the corpus's ways to start one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewNode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "releaseVersion")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub network: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_identity_seed: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enable_signing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_type: Option<KeyType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buckets: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_dir: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kms: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enable_searchable_encryption: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_type: Option<NodeClient>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_gate: Option<IndexGate>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub with_identity: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_document_acp_test: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for NewNode {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, _state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let mut probe = self.clone();
        probe.common = Common::default();
        probe.expected_data = Value::Null;
        probe.expected_error = None;
        if probe != Self::default() {
            return Err(Failure::unsupported(Self::name()));
        }
        Ok(json!({}))
    }
}

impl Action for NewNode {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
