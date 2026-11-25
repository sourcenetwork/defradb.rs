use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `collection.delete` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCollection {
    pub active_only: bool,
    pub names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for DeleteCollection {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    /// In a transaction through the node's runner, which deletes only in the
    /// transaction the context carries; otherwise straight on the database,
    /// as the HTTP API does.
    fn run(&self, state: &State) -> Result<Value, Failure> {
        let identity = self.common.client_identity(Self::name())?;
        let node = state.node_on(self.common.node())?;
        let context = state.context_as(self.txn, identity);
        let names = self.names.clone();
        let deleted = match self.txn {
            Some(_) => state
                .block_on(
                    node.runner()
                        .delete_collections_as(&context, names, self.active_only),
                )
                .map_err(|e| e.to_string()),
            None => state
                .block_on(
                    node.database()
                        .delete_collections_as(&context, names, self.active_only),
                )
                .map_err(|e| e.to_string()),
        };
        deleted.map(|()| json!({})).map_err(Failure::operation)
    }
}

impl Action for DeleteCollection {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for DeleteCollection {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            if action.txn.is_some() {
                return Err(Failure::unsupported(format!(
                    "{} in a transaction over HTTP",
                    Self::name()
                )));
            }
            let identity = action.common.client_identity(Self::name())?;
            let (names, active_only) = (action.names.clone(), action.active_only);
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.collection_delete(&names, active_only).await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for DeleteCollection {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let names = action.names.join(",");
            let mut args = vec!["collection", "delete", names.as_str()];
            if action.active_only {
                args.push("--active-only");
            }
            state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
