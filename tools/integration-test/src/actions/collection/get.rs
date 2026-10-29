use crate::scenario::{Common, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::prelude::*;

/// Lists the collection versions a node holds, optionally filtered.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCollections {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub get_inactive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "versionID")]
    pub version_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "collectionID")]
    pub collection_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub filtered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for GetCollections {
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

impl Action for GetCollections {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
