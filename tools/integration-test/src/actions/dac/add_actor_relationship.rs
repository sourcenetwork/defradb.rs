use super::relationship::{Change, Relationship};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::prelude::*;

/// The `dac.addActorRelationship` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDACActorRelationship {
    pub collection: String,
    #[serde(rename = "docID")]
    pub doc_id: Text,
    pub relation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_identity_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_identity_seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddDACActorRelationship {
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
        self.relationship()?.in_process(state)
    }
}

impl AddDACActorRelationship {
    fn relationship(&self) -> Result<Relationship<'_>, Failure> {
        let target = crate::actions::nac::target_actor(
            Self::name(),
            self.target_actor.as_deref(),
            self.target_identity_seed,
            self.target_identity_node.as_deref(),
        )?;
        Ok(Relationship {
            change: Change::Add,
            op: Self::name(),
            common: &self.common,
            collection: &self.collection,
            doc_id: self.doc_id.as_str(),
            relation: &self.relation,
            target,
        })
    }
}

impl Action for AddDACActorRelationship {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddDACActorRelationship {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.relationship()?.http(state)
        });
    }
}

impl CliRun for AddDACActorRelationship {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.relationship()?.cli(state)
        });
    }
}
