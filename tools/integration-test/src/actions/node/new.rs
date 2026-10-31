use crate::actions::shared::{IndexGate, KeyType, NodeClient};
use std::sync::Arc;

use crate::nodes::NodeSpec;
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
    /// A Rust node that speaks Go Shinzo's wire.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shinzo_wire: bool,
    /// A Rust node under every client, as a Shinzo-wire node is; the Go
    /// clients reach it over HTTP. For a Rust peer with the wire off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rust: bool,
    /// A Go node built from the Shinzo pin (`DEFRA_GO_SHINZO_BINARY`),
    /// whatever the test's client: the Go side of a mixed cluster. Actions
    /// aimed at it run as the go-shinzo client.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub go_shinzo: bool,
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

    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let mut probe = self.clone();
        probe.common = Common::default();
        probe.expected_data = Value::Null;
        probe.expected_error = None;
        probe.network = false;
        probe.database_dir = None;
        probe.shinzo_wire = false;
        probe.rust = false;
        probe.go_shinzo = false;
        if probe != Self::default() {
            let options = serde_json::to_string(&probe).unwrap_or_default();
            return Err(Failure::unsupported(format!("{} {options}", Self::name())));
        }
        let dir = match self.database_dir {
            Some(_) => Some(Arc::new(tempfile::tempdir().map_err(|e| {
                Failure::operation(format!("could not make the node's directory: {e}"))
            })?)),
            None => None,
        };
        let spec = NodeSpec {
            network: self.network,
            dir,
            listen: Vec::new(),
            shinzo_wire: self.shinzo_wire,
            rust: self.rust,
            go_shinzo: self.go_shinzo,
        };
        state.start_node(self.common.node(), spec);
        Ok(json!({}))
    }
}

impl Action for NewNode {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
