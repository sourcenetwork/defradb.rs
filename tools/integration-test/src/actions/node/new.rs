use crate::actions::shared::{IndexGate, KeyType, NodeClient};
use crate::client_type::ClientType;
use std::sync::Arc;
use std::time::Duration;

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
    /// Kept so corpus scenarios that set it still parse. It changes nothing:
    /// every node gets Go's `NodeIdentity(i)` and runs identity-only unless
    /// it signs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub with_identity: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_document_acp_test: bool,
    /// A Rust node with the given admission limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admission: Option<AdmissionSpec>,
    /// A Rust node whose network is iroh rather than libp2p.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub iroh: bool,
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
        probe.iroh = false;
        probe.node_identity_seed = None;
        probe.enable_signing = false;
        probe.key_type = None;
        probe.with_identity = false;
        probe.client_type = None;
        probe.enable_searchable_encryption = false;
        // Rust nodes always run pubsub KMS, so this only says what Go's
        // harness sets up for them.
        probe.kms = false;
        (probe.ttl_millis, probe.tick_millis, probe.buckets) = (None, None, None);
        probe.index_gate = None;
        if probe != Self::default() {
            let options = serde_json::to_string(&probe).unwrap_or_default();
            return Err(Failure::unsupported(format!("{} {options}", Self::name())));
        }
        self.check_client(state.client_type())?;
        // Go gives node `i` its `NodeIdentity(i)` whether or not it signs,
        // and counts every node it starts, one with its own seed included.
        // `withIdentity` asks for no more than that.
        let go_index = state.next_go_node_index();
        let node_identity = self.node_identity_seed.or(Some(go_index));
        let signing = (self.enable_signing || self.key_type.is_some())
            .then(|| self.key_type.unwrap_or_default());
        let txn_ttl = self.txn_ttl(state.client_type())?;
        let index_gate = self.index_gate(state.client_type())?;
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
            iroh_network: self.iroh,
            node_identity,
            go_index: Some(state.next_go_node_index()),
            signing,
            txn_ttl,
            se_key: self.searchable_encryption_key()?,
            index_gate,
            ..Default::default()
        };
        state.start_node(self.common.node(), spec);
        Ok(json!({}))
    }
}

impl NewNode {
    /// A key of the node's own when the scenario turns searchable encryption
    /// on, as Go's harness generates one per node.
    fn searchable_encryption_key(&self) -> Result<Option<[u8; 32]>, Failure> {
        if !self.enable_searchable_encryption {
            return Ok(None);
        }
        let key = crypto::generate_aes256()
            .map_err(|e| Failure::operation(format!("searchable encryption key: {e}")))?;
        key.try_into()
            .map(Some)
            .map_err(|_| Failure::operation("an AES-256 key is 32 bytes".into()))
    }

    /// The gate the node's index backfills pass, Go's `db.IndexBuildGate`. It
    /// is a hook in the node's process, so a Go node cannot take one.
    fn index_gate(
        &self,
        client: ClientType,
    ) -> Result<Option<Arc<crate::index_gate::TestIndexGate>>, Failure> {
        let Some(kind) = self.index_gate else {
            return Ok(None);
        };
        if client == ClientType::GoHttp {
            return Err(Failure::unsupported(format!(
                "{} argument indexGate under {client:?}: the gate is a hook in the node's process",
                Self::name()
            )));
        }
        Ok(Some(Arc::new(crate::index_gate::TestIndexGate::new(kind))))
    }

    /// How long an idle explicit transaction lives and how often the node
    /// sweeps for idle ones, Go's `TxnTTL` and `TxnTTLTick`. Rust sweeps on
    /// its tick, so Go's bucket count has nothing to set. Every Rust node
    /// sweeps, with or without an HTTP server.
    fn txn_ttl(&self, client: ClientType) -> Result<Option<(Duration, Duration)>, Failure> {
        let Some(ttl) = self.ttl_millis else {
            return Ok(None);
        };
        if client == ClientType::GoHttp {
            return Err(Failure::unsupported(format!(
                "{} argument ttlMillis under {client:?}",
                Self::name()
            )));
        }
        let tick = self.tick_millis.unwrap_or(ttl);
        Ok(Some((
            Duration::from_millis(ttl),
            Duration::from_millis(tick),
        )))
    }

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
