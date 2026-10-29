use crate::actions::shared::Lens;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::Text;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Patches the collection definitions, optionally with a lens that migrates the data, and optionally while a document is being added.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchCollection {
    pub patch: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens: Option<Lens>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[serde(rename = "async")]
    pub r#async: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "document")]
    pub doc_map: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for PatchCollection {
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
        let migration = self.lens.as_ref().map(Lens::to_wire);
        state
            .block_on(state.node().patch_collection_as(
                &state.context(self.txn),
                self.patch.as_str(),
                migration.as_ref(),
            ))
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl PatchCollection {
    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.plain(Self::name())?;
        let unsupported = [
            ("async", self.r#async),
            ("collection", self.collection.is_some()),
            ("document", self.doc_map.is_some()),
            ("txn", self.txn.is_some()),
        ];
        match unsupported.iter().find(|(_, set)| *set) {
            Some((name, _)) => Err(Failure::unsupported(format!(
                "{} argument {name}",
                Self::name()
            ))),
            None => Ok(()),
        }
    }
}

impl Action for PatchCollection {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for PatchCollection {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let txn = state.txn_id(action.txn);
            let patch = action.patch.as_str().to_owned();
            let migration = action.lens.as_ref().map(Lens::to_wire);
            state
                .http(|client| async move {
                    client
                        .patch_collection(txn.as_deref(), &patch, migration.as_ref())
                        .await
                })
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for PatchCollection {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let migration = action.lens.as_ref().map(|lens| lens.to_wire().to_string());
            let mut args = vec!["collection", "patch", action.patch.as_str()];
            args.extend(migration.as_deref());
            state
                .defra_client_in(action.txn, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
