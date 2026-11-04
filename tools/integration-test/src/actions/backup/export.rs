use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `backup.export` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBackup {
    pub collections: Vec<String>,
    pub format: String,
    pub missing_file: bool,
    pub pretty: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ExportBackup {
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
            .block_on(db::backup::export_database_as(
                &state.context(None),
                node.database(),
                node.runner(),
                &self.collections,
                self.pretty,
            ))
            .map(content)
            .map_err(Failure::operation)
    }
}

fn content(text: String) -> Value {
    json!({ "content": text })
}

impl ExportBackup {
    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.plain(Self::name())?;
        if !matches!(self.format.as_str(), "" | "json") {
            return Err(Failure::unsupported(format!(
                "{} argument format",
                Self::name()
            )));
        }
        if self.missing_file {
            return Err(Failure::unsupported(format!(
                "{} argument missingFile",
                Self::name()
            )));
        }
        Ok(())
    }
}

impl Action for ExportBackup {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ExportBackup {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let collections = Some(action.collections.clone()).filter(|names| !names.is_empty());
            let pretty = action.pretty;
            state
                .http_on(action.common.node(), |client| async move {
                    client.backup_export(collections.as_deref(), pretty).await
                })?
                .map(content)
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for ExportBackup {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let dir = tempfile::tempdir().expect("a directory for the backup file");
            let file = dir.path().join("backup.json");
            let file = file.to_string_lossy().into_owned();
            let mut args = vec!["backup", "export", file.as_str()];
            for name in &action.collections {
                args.extend(["--collections", name.as_str()]);
            }
            if action.pretty {
                args.push("--pretty");
            }
            state
                .defra_client_development(action.common.node(), &args)
                .map_err(Failure::operation)?;
            std::fs::read_to_string(&file)
                .map(content)
                .map_err(|e| Failure::operation(format!("read the exported backup: {e}")))
        });
    }
}
