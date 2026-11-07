use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
        let (requester, target) = self.actors()?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .add_dac_actor_relationship(
                        &requester,
                        &self.collection,
                        self.doc_id.as_str(),
                        &self.relation,
                        &target,
                    ),
            )
            .map(|existed_already| json!({ "existedAlready": existed_already }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl AddDACActorRelationship {
    /// The DIDs of the actor granting and the actor granted.
    fn actors(&self) -> Result<(String, String), Failure> {
        let requester = match self.common.client_identity(Self::name())? {
            Some(seed) => crate::scenario::identity::did(seed),
            None => {
                return Err(Failure::unsupported(format!(
                    "{} without an identity",
                    Self::name()
                )))
            }
        };
        if self.target_identity_node.is_some() {
            return Err(Failure::unsupported(format!(
                "{} argument targetIdentityNode",
                Self::name()
            )));
        }
        let target = match (&self.target_actor, self.target_identity_seed) {
            (Some(actor), _) => actor.clone(),
            (None, Some(seed)) => crate::scenario::identity::did(seed),
            (None, None) => {
                return Err(Failure::operation(format!(
                    "{} names no target actor",
                    Self::name()
                )))
            }
        };
        Ok((requester, target))
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
            let (_, target) = action.actors()?;
            let identity = action.common.client_identity(Self::name())?;
            let (collection, doc_id, relation) = (
                action.collection.clone(),
                action.doc_id.as_str().to_owned(),
                action.relation.clone(),
            );
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client
                        .acp_doc_relationship_add(&collection, &doc_id, &relation, &target)
                        .await
                })?
                .map_err(|e| Failure::operation(e.to_string()))
                .and_then(|printed| existed_already(&printed))
        });
    }
}

impl CliRun for AddDACActorRelationship {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let (_, target) = action.actors()?;
            let identity = action.common.client_identity(Self::name())?;
            let args = [
                "acp",
                "document",
                "relationship",
                "add",
                "--collection",
                &action.collection,
                "--docID",
                action.doc_id.as_str(),
                "--relation",
                &action.relation,
                "--actor",
                &target,
            ];
            state
                .defra_client_as(action.common.node(), None, identity, &args)
                .map_err(Failure::operation)
                .and_then(|printed| existed_already(&printed))
        });
    }
}

/// Go's `existedAlready` from what the server printed.
fn existed_already(printed: &Value) -> Result<Value, Failure> {
    ["ExistedAlready", "existedAlready", "existed_already"]
        .iter()
        .find_map(|key| printed.get(*key).and_then(Value::as_bool))
        .map(|existed| json!({ "existedAlready": existed }))
        .ok_or_else(|| Failure::operation(format!("no existedAlready in {printed}")))
}
