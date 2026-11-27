use std::sync::Arc;

use crate::client_type::ClientType;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `collection.pruneHistory` op: `DB::prune_history_as` in process,
/// `POST /collections/{name}/history/prune` over HTTP, and
/// `client collection prune-history` through the CLI. Go has no history
/// retention, so go-http does not run it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneHistory {
    pub collection: String,
    /// The explicit transaction the prune is sent in, which the node refuses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    /// How many commits the prune must report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for PruneHistory {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    /// On the database's pool, as the HTTP API runs it.
    fn run(&self, state: &State) -> Result<Value, Failure> {
        let identity = self.common.client_identity(Self::name())?;
        let node = state.node_on(self.common.node())?;
        let database = Arc::clone(node.database());
        let context = state.context_as(self.txn, identity);
        let name = self.collection.clone();
        let pruned = state
            .block_on(workers::run_on(
                node.database().pool(),
                workers::Target::LeastLoaded,
                move || async move { database.prune_history_as(&context, &name).await },
            ))
            .map_err(|error| Failure::operation(error.to_string()))?
            .map_err(|error| Failure::operation(error.to_string()))?;
        self.check_count(pruned)
    }
}

impl PruneHistory {
    fn check_count(&self, pruned: usize) -> Result<Value, Failure> {
        match self.expected_count {
            Some(expected) if pruned != expected => Err(Failure::operation(format!(
                "expected {expected} pruned commits; got {pruned}"
            ))),
            _ => Ok(json!({})),
        }
    }

    /// The count a wire response carries.
    fn wire_count(response: &Value) -> Result<usize, Failure> {
        response["count"]
            .as_u64()
            .map(|count| count as usize)
            .ok_or_else(|| {
                Failure::operation(format!("prune response without a count: {response}"))
            })
    }
}

impl HttpRun for PruneHistory {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            if state.client_type() == ClientType::GoHttp {
                return Err(Failure::unsupported(
                    "history prune under GoHttp: Go has no history retention",
                ));
            }
            let identity = action.common.client_identity(Self::name())?;
            let txn = state.txn_id(action.txn);
            let collection = action.collection.clone();
            let response = state
                .http_as(action.common.node(), identity, |client| async move {
                    client
                        .collection_prune_history(txn.as_deref(), &collection)
                        .await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            action.check_count(Self::wire_count(&response)?)
        });
    }
}

impl CliRun for PruneHistory {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let args = [
                "collection",
                "--collection-name",
                &action.collection,
                "prune-history",
            ];
            let response = state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map_err(Failure::operation)?;
            action.check_count(Self::wire_count(&response)?)
        });
    }
}

impl Action for PruneHistory {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
