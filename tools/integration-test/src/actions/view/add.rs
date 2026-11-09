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
        self.unsupported(&[("txn", self.txn.is_some())])?;
        state
            .block_on(state.node_on(self.common.node())?.add_view_as(
                &state.context_as(None, identity),
                self.query.as_str(),
                self.sdl.as_str(),
            ))
            .map(|()| json!({}))
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
    /// Fails on the first argument in `set` that is given, and on a lens
    /// transform, which only the CLI sends yet.
    fn unsupported(&self, set: &[(&str, bool)]) -> Result<(), Failure> {
        let transform = [("transformCID", self.transform_cid.is_some())];
        match set.iter().chain(&transform).find(|(_, given)| *given) {
            Some((name, _)) => Err(Failure::unsupported(format!(
                "{} argument {name}",
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
            action.unsupported(&[("txn", action.txn.is_some())])?;
            let (query, sdl) = (
                action.query.as_str().to_owned(),
                action.sdl.as_str().to_owned(),
            );
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.view_add(&query, &sdl, None).await
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
