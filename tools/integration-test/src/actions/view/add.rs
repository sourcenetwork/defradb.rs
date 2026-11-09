use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `view.add` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddView {
    pub query: Text,
    pub sdl: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(rename = "transformCID")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform_cid: Option<Text>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddView {
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
        self.unsupported_txn()?;
        state
            .block_on(state.node_on(self.common.node())?.add_view_as(
                &state.context_as(None, identity),
                self.query.as_str(),
                self.sdl.as_str(),
                self.transform_cid.as_ref().map(Text::as_str),
            ))
            .map(|_| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for AddView {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl AddView {
    /// Fails on a transaction, which the core's view creation does not run in.
    fn unsupported_txn(&self) -> Result<(), Failure> {
        match self.txn {
            Some(_) => Err(Failure::unsupported(format!(
                "{} argument txn",
                Self::name()
            ))),
            None => Ok(()),
        }
    }
}

impl HttpRun for AddView {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            action.unsupported_txn()?;
            let (query, sdl) = (
                action.query.as_str().to_owned(),
                action.sdl.as_str().to_owned(),
            );
            let transform = action
                .transform_cid
                .as_ref()
                .map(|cid| cid.as_str().to_owned());
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.view_add(&query, &sdl, transform.as_deref()).await
                })?
                .map(|_| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for AddView {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let mut args = vec![
                "view",
                "add",
                "--query",
                action.query.as_str(),
                "--sdl",
                action.sdl.as_str(),
            ];
            if let Some(cid) = &action.transform_cid {
                args.extend(["--lens-cid", cid.as_str()]);
            }
            state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
