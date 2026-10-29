use crate::actions::shared::Lens;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `migration.set` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureMigration {
    #[serde(rename = "destinationVersionID")]
    pub destination_version_id: Text,
    pub lens: Lens,
    #[serde(rename = "sourceVersionID")]
    pub source_version_id: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ConfigureMigration {
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
        self.common.plain(Self::name())?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .set_migration_as(&state.context(self.txn), &self.config()),
            )
            .map(|id| json!({ "lensID": id }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl ConfigureMigration {
    /// Go's `LensConfig`.
    fn config(&self) -> Value {
        let mut config = self.lens.to_wire();
        config["SourceCollectionVersionID"] = json!(self.source_version_id.as_str());
        config["DestinationCollectionVersionID"] = json!(self.destination_version_id.as_str());
        config
    }
}

impl Action for ConfigureMigration {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ConfigureMigration {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let (txn, config) = (state.txn_id(action.txn), action.config());
            state
                .http_on(action.common.node(), |client| async move {
                    client.set_migration(txn.as_deref(), &config).await
                })?
                .map(|id| json!({ "lensID": id }))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for ConfigureMigration {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let lens = action.lens.to_wire().to_string();
            let args = [
                "lens",
                "set",
                action.source_version_id.as_str(),
                action.destination_version_id.as_str(),
                &lens,
            ];
            let printed = state
                .defra_client_on(action.common.node(), action.txn, &args)
                .map_err(Failure::operation)?;
            Ok(json!({ "lensID": printed["lensId"] }))
        });
    }
}
