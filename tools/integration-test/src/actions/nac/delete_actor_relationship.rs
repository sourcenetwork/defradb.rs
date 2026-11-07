use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `nac.deleteActorRelationship` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteNACActorRelationship {
    pub relation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_identity_seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl DeleteNACActorRelationship {
    fn target(&self) -> Result<String, Failure> {
        super::target_actor(
            Self::name(),
            self.target_actor.as_deref(),
            self.target_identity_seed,
            None,
        )
    }
}

/// Go's result as the scenarios read it.
fn result(found: bool) -> Value {
    json!({ "recordFound": found })
}

impl Op for DeleteNACActorRelationship {
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
        let target = self.target()?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .delete_nac_actor_relationship_as(
                        &state.context_as(None, identity),
                        &self.relation,
                        &target,
                    ),
            )
            .map(result)
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for DeleteNACActorRelationship {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for DeleteNACActorRelationship {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let (relation, target) = (action.relation.clone(), action.target()?);
            let printed = state
                .http_as(action.common.node(), identity, |client| async move {
                    client.nac_remove_relationship(&relation, &target).await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            printed_result(printed)
        });
    }
}

impl CliRun for DeleteNACActorRelationship {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let target = action.target()?;
            let args = [
                "acp",
                "node",
                "relationship",
                "delete",
                "--relation",
                action.relation.as_str(),
                "--actor",
                target.as_str(),
            ];
            let printed = state
                .defra_client_as(action.common.node(), None, identity, &args)
                .map_err(Failure::operation)?;
            printed_result(printed)
        });
    }
}

/// Go's `RecordFound` as the HTTP API and the CLI print it.
fn printed_result(printed: Value) -> Result<Value, Failure> {
    match printed.get("RecordFound").and_then(Value::as_bool) {
        Some(found) => Ok(result(found)),
        None => Err(Failure::operation(format!("no RecordFound in {printed}"))),
    }
}
