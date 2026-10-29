use crate::actions::shared::Lens;
use crate::scenario::Text;
use crate::scenario::{Common, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
}

impl Action for PatchCollection {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
