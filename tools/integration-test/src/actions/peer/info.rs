use crate::actions::peer::support::{addresses_of, failed};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Reads a node's peer information: its address, or the validated information, or only its peer ID.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub validated: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub id_only: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for PeerInfo {
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
        self.check_arguments()?;
        let node = state.node_on(self.common.node())?;
        let addresses = state
            .block_on(node.peer_info_as(&state.context_as(None, self.common.identity_seed)))
            .map_err(failed)?;
        Ok(self.shape(addresses))
    }
}

impl PeerInfo {
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.client_identity(Self::name())?;
        match self.validated {
            true => Err(Failure::unsupported("PeerInfo argument validated")),
            false => Ok(()),
        }
    }

    /// A step that only names the addresses for later actions, as Go's
    /// `{{.Peer0_Address0}}` template does: Go reads them from the test's own
    /// view of the node, never through the client, so node access control
    /// does not apply.
    fn captures_only(&self) -> bool {
        !self.common.capture.is_empty()
            && self.expected_data.is_null()
            && self.expected_error.is_none()
            && !self.id_only
    }

    fn shape(&self, addresses: Vec<String>) -> Value {
        match self.id_only {
            true => json!({ "peerID": addresses.first().map(|a| crate::nodes::peer_id(a)) }),
            false => json!({ "addresses": addresses }),
        }
    }
}

impl Action for PeerInfo {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for PeerInfo {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            if action.captures_only() {
                return Ok(action.shape(addresses_of(state, &[action.common.node()])?));
            }
            let addresses = state
                .http_as(
                    action.common.node(),
                    action.common.identity_seed,
                    |client| async move { client.peer_info().await },
                )?
                .map_err(failed)?;
            Ok(action.shape(addresses))
        });
    }
}

impl CliRun for PeerInfo {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            if action.captures_only() {
                return Ok(action.shape(addresses_of(state, &[action.common.node()])?));
            }
            let printed = state
                .defra_client_as(
                    action.common.node(),
                    None,
                    action.common.identity_seed,
                    &["p2p", "info"],
                )
                .map_err(Failure::operation)?;
            let addresses = serde_json::from_value(printed).map_err(failed)?;
            Ok(action.shape(addresses))
        });
    }
}
