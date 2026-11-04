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
        self.check_arguments()?;
        let node = state.node_on(self.common.node())?;
        state
            .block_on(db::backup::import_database_as(
                &state.context(None),
                node.database(),
                node.runner(),
                self.import_content.as_str(),
            ))
            .map(|_| json!({}))
            .map_err(Failure::operation)
    }
}

impl ImportBackup {
    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.plain(Self::name())?;
        match self.missing_file {
            true => Err(Failure::unsupported(format!(
                "{} argument missingFile",
                Self::name()
            ))),
            false => Ok(()),
        }
    }
}

impl Action for ImportBackup {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ImportBackup {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let content = action.import_content.as_str().to_owned();
            state
                .http_on(action.common.node(), |client| async move {
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
            action.check_arguments()?;
            let dir = tempfile::tempdir().expect("a directory for the backup file");
            let file = dir.path().join("backup.json");
            std::fs::write(&file, action.import_content.as_str())
                .map_err(|e| Failure::operation(format!("write the backup file: {e}")))?;
            let file = file.to_string_lossy().into_owned();
            state
                .defra_client_development(action.common.node(), &["backup", "import", &file])
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
