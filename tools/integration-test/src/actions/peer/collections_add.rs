use crate::actions::peer::subscription_seen::subscribed_everywhere;
use crate::actions::peer::support::failed;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `peer.collections.add` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCollectionSubscription {
    pub collections: Vec<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddCollectionSubscription {
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
        let names = self.collections.clone();
        let node = state.node_on(self.common.node())?;
        subscribed_everywhere(state, self.common.node(), || {
            state
                .block_on(node.add_p2p_collections_as(&state.context_as(None, identity), &names))
                .map_err(failed)?;
            Ok(json!({}))
        })
    }
}

impl Action for AddCollectionSubscription {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddCollectionSubscription {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let names = action.collections.clone();
            subscribed_everywhere(state, action.common.node(), || {
                state
                    .http_as(action.common.node(), identity, |client| async move {
                        client.add_p2p_collections(&names).await
                    })?
                    .map_err(failed)?;
                Ok(json!({}))
            })
        });
    }
}

impl CliRun for AddCollectionSubscription {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let names = action.collections.join(",");
            subscribed_everywhere(state, action.common.node(), || {
                state
                    .defra_client_as(
                        action.common.node(),
                        None,
                        identity,
                        &["p2p", "collection", "add", &names],
                    )
                    .map_err(Failure::operation)?;
                Ok(json!({}))
            })
        });
    }
}
