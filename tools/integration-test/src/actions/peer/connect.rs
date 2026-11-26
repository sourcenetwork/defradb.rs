use crate::actions::peer::support::{addresses_of, failed};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `peer.connect` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectPeers {
    pub target: String,
    /// Dial the target by its peer id alone, with none of its addresses:
    /// only a hub with pinc's relay role can find it then.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_id: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl ConnectPeers {
    /// What the dial names: the target's addresses, or with `by_id` the
    /// peer id each one starts with. A Go node's libp2p address has no
    /// pinc id to dial alone, so a by-id dial to or from one is a Rust
    /// node's op only.
    fn addresses(&self, state: &State) -> Result<Vec<String>, Failure> {
        let mut addresses = addresses_of(state, &[self.target.as_str()])?;
        if self.by_id {
            for name in [self.common.node(), self.target.as_str()] {
                if state.node(name)?.is_go() {
                    return Err(Failure::unsupported(format!(
                        "{} argument byId with Go node {name}: only a pinc node dials by id alone",
                        Self::name()
                    )));
                }
            }
            for address in &mut addresses {
                address.truncate(address.find('@').unwrap_or(address.len()));
            }
            addresses.dedup();
        }
        Ok(addresses)
    }
}

impl Op for ConnectPeers {
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
        let addresses = self.addresses(state)?;
        let node = state.node_on(self.common.node())?;
        state
            .block_on(node.connect_as(&state.context_as(None, identity), &addresses))
            .map_err(failed)?;
        Ok(json!({}))
    }
}

impl Action for ConnectPeers {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ConnectPeers {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let addresses = action.addresses(state)?;
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.connect(&addresses).await
                })?
                .map_err(failed)?;
            Ok(json!({}))
        });
    }
}

impl CliRun for ConnectPeers {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let addresses = action.addresses(state)?;
            state
                .defra_client_as(
                    action.common.node(),
                    None,
                    identity,
                    &[
                        &["p2p", "connect"],
                        addresses
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .as_slice(),
                    ]
                    .concat(),
                )
                .map_err(Failure::operation)?;
            Ok(json!({}))
        });
    }
}
