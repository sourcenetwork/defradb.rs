use super::file::{refuse_go, BackupFile};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `backup.import` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBackup {
    #[serde(rename = "content")]
    pub import_content: Text,
    pub missing_file: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ImportBackup {
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
        let content = self.through_file()?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .import_backup_as(&state.context_as(None, identity), &content),
            )
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for ImportBackup {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl ImportBackup {
    /// The backup as the node reads it back from its file.
    fn through_file(&self) -> Result<String, Failure> {
        let file = BackupFile::new(false)?;
        if !self.missing_file {
            file.write(self.import_content.as_str())?;
        }
        file.read()
    }
}

impl HttpRun for ImportBackup {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            refuse_go(state, Self::name())?;
            let content = action.through_file()?;
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.backup_import(&content).await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for ImportBackup {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let file = BackupFile::new(false)?;
            if !action.missing_file {
                file.write(action.import_content.as_str())?;
            }
            let args = ["--development", "backup", "import", file.as_str()];
            state
                .defra_client_as(action.common.node(), None, identity, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
