use crate::scenario::{Common, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::prelude::*;

/// Adds a document to a collection: one document (`doc_map`) or several (`docs`), encrypted or not.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDoc {
    pub collection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "document")]
    pub doc_map: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "documents")]
    pub docs: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "encryptDoc")]
    pub is_doc_encrypted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_fields: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddDoc {
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

impl Action for AddDoc {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
