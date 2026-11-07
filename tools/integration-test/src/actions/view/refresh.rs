use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `view.refresh` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshViews {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for RefreshViews {
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
        if self.txn.is_some() {
            return Err(Failure::unsupported(format!(
                "{} argument txn",
                Self::name()
            )));
        }
        let selector = db_selector(self.collection_name.clone());
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .refresh_views_as(&state.context_as(None, identity), selector),
            )
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for RefreshViews {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

fn db_selector(name: Option<String>) -> schema::CollectionSelector {
    schema::CollectionSelector {
        names: name.map(|name| vec![name]),
        ..Default::default()
    }
}

impl HttpRun for RefreshViews {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            if action.txn.is_some() {
                return Err(Failure::unsupported(format!(
                    "{} argument txn",
                    Self::name()
                )));
            }
            let selectors = cli::commands::client::http_client::ViewRefreshSelectors {
                name: action.collection_name.clone(),
                collection_id: None,
                version_id: None,
                get_inactive: false,
            };
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.view_refresh(&selectors).await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for RefreshViews {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let mut args = vec!["view", "refresh"];
            if let Some(name) = &action.collection_name {
                args.extend(["--collection-name", name.as_str()]);
            }
            state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
