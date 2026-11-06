use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use super::{in_process, peer_id};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// This iroh node admits `target`, through `Node::allow_p2p_peer`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowStreamPeer {
    pub target: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

/// This iroh node revokes `target`, through `Node::deny_p2p_peer`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DenyStreamPeer {
    pub target: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

fn admit(state: &State, node: &str, target: &str, allow: bool, op: &str) -> Result<Value, Failure> {
    let node = in_process(state, node, op)?;
    let target = in_process(state, target, op)?;
    let peer = peer_id(state, &target)?;
    let done = match allow {
        true => state.block_on(node.allow_p2p_peer(&peer)),
        false => state.block_on(node.deny_p2p_peer(&peer)),
    };
    done.map_err(|e| Failure::operation(e.to_string()))?;
    Ok(json!({}))
}

macro_rules! admission_op {
    ($action:ident, $allow:expr) => {
        impl Op for $action {
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
                self.common.plain(Self::name())?;
                admit(
                    state,
                    self.common.node(),
                    &self.target,
                    $allow,
                    Self::name(),
                )
            }
        }

        impl Action for $action {
            type TState = State;

            fn execute(&self, state: &mut State) {
                crate::scenario::execute(self, state);
            }
        }
    };
}

admission_op!(AllowStreamPeer, true);
admission_op!(DenyStreamPeer, false);
