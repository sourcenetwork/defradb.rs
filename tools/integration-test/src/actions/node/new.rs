use crate::actions::shared::{IndexGate, KeyType, NodeClient};
use crate::client_type::ClientType;
use std::sync::Arc;

use crate::nodes::NodeSpec;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// A Rust receiver's admission limits: its pending-DAG map and its per-peer
/// rate bucket (tokens, and tokens refilled per second).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionSpec {
    pub max_pending_dags: usize,
    pub rate_limit_burst: u32,
    pub rate_limit_rate: f64,
    /// The byte budget for one peer's deferred gossip heads; `None` keeps
    /// the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferral_peer_bytes: Option<u64>,
}

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
    /// A Rust node with the given admission limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admission: Option<AdmissionSpec>,
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
        probe.admission = None;
        probe.node_identity_seed = None;
        probe.enable_signing = false;
        probe.key_type = None;
        probe.with_identity = false;
        probe.client_type = None;
        if probe != Self::default() {
            let options = serde_json::to_string(&probe).unwrap_or_default();
            return Err(Failure::unsupported(format!("{} {options}", Self::name())));
        }
        self.check_client(state.client_type())?;
        let node_identity = self
            .node_identity_seed
            .or(self.with_identity.then_some(u64::MAX - 1));
        let signing = (self.enable_signing || self.key_type.is_some())
            .then(|| self.key_type.unwrap_or_default());
        if state.client_type() == ClientType::GoHttp
            && (node_identity.is_some() || signing.is_some())
        {
            return Err(Failure::unsupported(
                "a node identity or signing key on a Go node",
            ));
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
            admission: self.admission.clone(),
            node_identity,
            signing,
            ..Default::default()
        };
        state.start_node(self.common.node(), spec);
        Ok(json!({}))
    }
}

impl NewNode {
    /// A scenario pinned to one client runs only under that client, as Go's
    /// harness skips it under the others.
    fn check_client(&self, running: ClientType) -> Result<(), Failure> {
        let fits = match self.client_type {
            None => true,
            Some(NodeClient::Http) => matches!(running, ClientType::RustHttp | ClientType::GoHttp),
            Some(NodeClient::Cli) => running == ClientType::Cli,
            Some(NodeClient::C) => false,
        };
        match fits {
            true => Ok(()),
            false => Err(Failure::unsupported(format!(
                "{} for the {:?} client under {running:?}",
                Self::name(),
                self.client_type.unwrap_or_default()
            ))),
        }
    }
}

impl Action for NewNode {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
