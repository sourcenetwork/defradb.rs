use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::action::ActionInstance;
use testo::prelude::*;

use super::Actions;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// Runs its actions at the same time. Each child expects its outcome as
/// `{"result": .., "error": ..}`, and a child that fails does not fail the
/// test. A group the corpus keeps as raw `children` is not run yet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parallel {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Actions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Value>>,
    /// The group is expected to still be running when the test ends.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub expect_pending: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for Parallel {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, _state: &State) -> Result<Value, Failure> {
        Err(Failure::unsupported(Self::name()))
    }
}

impl Action for Parallel {
    type TState = State;

    fn execute(&self, state: &mut State) {
        if self.children.is_some() {
            crate::scenario::execute(self, state);
            return;
        }
        let children: Vec<ActionInstance<Actions>> = self
            .actions
            .iter()
            .cloned()
            .map(ActionInstance::Async)
            .collect();
        drop(testo::action::execute(state.clone(), &children));
    }
}
