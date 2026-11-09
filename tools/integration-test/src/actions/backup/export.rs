use super::file::{refuse_go, BackupFile};
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
        let identity = self.common.client_identity(Self::name())?;
        let content = state
            .block_on(state.node_on(self.common.node())?.export_backup_as(
                &state.context_as(None, identity),
                &self.collections,
                self.pretty,
            ))
            .map_err(|e| Failure::operation(e.to_string()))?;
        self.through_file(&content)
    }
}

impl Action for ExportBackup {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl ExportBackup {
    /// The export as the file it was written to.
    fn through_file(&self, content: &str) -> Result<Value, Failure> {
        let file = BackupFile::new(self.missing_file)?;
        file.write(content)?;
        Ok(json!({ "content": file.read()? }))
    }
}

impl HttpRun for ExportBackup {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            refuse_go(state, Self::name())?;
            let (collections, pretty) = (action.collections.clone(), action.pretty);
            let content = state
                .http_as(action.common.node(), identity, |client| async move {
                    let selected = (!collections.is_empty()).then_some(collections.as_slice());
                    client.backup_export(selected, pretty).await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            action.through_file(&content)
        });
    }
}

impl CliRun for ExportBackup {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let file = BackupFile::new(action.missing_file)?;
            let mut args = vec!["--development", "backup", "export", file.as_str()];
            for collection in &action.collections {
                args.extend(["--collections", collection.as_str()]);
            }
            if action.pretty {
                args.push("--pretty");
            }
            state
                .defra_client_as(action.common.node(), None, identity, &args)
                .map_err(Failure::operation)?;
            Ok(json!({ "content": file.read()? }))
        });
    }
}
