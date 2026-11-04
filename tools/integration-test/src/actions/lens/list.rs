use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use testo::prelude::*;

/// Lists the lenses a node holds: their metadata, or how many.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListLenses {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_count: Value,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ListLenses {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        match self.expected_data.is_null() {
            true => &self.expected_count,
            false => &self.expected_data,
        }
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let lenses = state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .list_lenses_as(&state.context(self.txn)),
            )
            .map_err(|e| Failure::operation(e.to_string()))?;
        Ok(self.shape(lenses))
    }
}

impl ListLenses {
    /// The listing as the scenario compares it: its count, or each lens's
    /// ID and modules. Go's harness compares each module's inverse flag and
    /// arguments.
    fn shape(&self, lenses: Map<String, Value>) -> Value {
        if !self.expected_count.is_null() {
            return json!({ "count": lenses.len() });
        }
        let lenses: Vec<Value> = lenses
            .into_iter()
            .map(|(id, lens)| {
                let modules: Vec<Value> = lens["Lenses"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|module| {
                        json!({
                            "Inverse": module.get("Inverse").cloned().unwrap_or(json!(false)),
                            "Arguments": module.get("Arguments").cloned().unwrap_or(Value::Null),
                        })
                    })
                    .collect();
                json!({ "lensID": id, "modules": modules })
            })
            .collect();
        json!({ "lenses": lenses })
    }
}

impl Action for ListLenses {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ListLenses {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let txn = state.txn_id(action.txn);
            let lenses = state
                .http_on(action.common.node(), |client| async move {
                    client.list_lenses(txn.as_deref()).await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            Ok(action.shape(lenses))
        });
    }
}

impl CliRun for ListLenses {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let printed = state
                .defra_client_on(action.common.node(), action.txn, &["lens", "list"])
                .map_err(Failure::operation)?;
            let lenses = match printed {
                Value::Object(lenses) => lenses,
                Value::Null => Map::new(),
                other => {
                    return Err(Failure::operation(format!(
                        "defra client lens list printed {other}"
                    )))
                }
            };
            Ok(action.shape(lenses))
        });
    }
}
