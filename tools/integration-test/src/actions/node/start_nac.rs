use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `node.startNAC` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Start {
    #[serde(rename = "enableNAC")]
    pub enable_nac: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_signing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_identity_seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for Start {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        let owner = self.common.client_identity(Self::name())?;
        let (node_identity, signing) = (self.node_identity_seed, self.enable_signing);
        // A node identity always signs here: the core has no identity that
        // does not sign, so Go's unsigned node with an identity can't run.
        if signing == Some(false) && node_identity.is_some() {
            return Err(Failure::unsupported(format!(
                "{} enableSigning false with a node identity",
                Self::name()
            )));
        }
        state.start_node_with_nac(self.common.node(), self.enable_nac, owner, |spec| {
            if let Some(seed) = node_identity {
                spec.node_identity = Some(seed);
            }
            match signing {
                Some(true) => spec.signing = Some(spec.signing.unwrap_or_default()),
                Some(false) => spec.signing = None,
                None => {}
            }
        })?;
        Ok(json!({}))
    }
}

impl Action for Start {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
