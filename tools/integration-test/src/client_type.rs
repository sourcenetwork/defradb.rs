use std::sync::Arc;

use defra_node::Node;
use serde::{Deserialize, Serialize};
use testo::prelude::*;

use crate::actions::{Actions, Connect};
use crate::test_case::State;

/// How a test reaches its database. The Go suite's `DEFRA_CLIENT_*` switches.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientType {
    /// Calls the node directly.
    InProcess,
}

impl ClientType {
    pub const ALL: [ClientType; 1] = [Self::InProcess];

    /// Starts a fresh database and points `state` at it.
    pub fn connect(self, state: &mut State) {
        match self {
            Self::InProcess => state.node = Some(Arc::new(start_node())),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::InProcess => "in-process",
        }
    }
}

impl Multiplier<Actions> for ClientType {
    const NAME: &str = "client-type";

    fn name(&self) -> &str {
        ClientType::name(*self)
    }

    fn apply(&self, actions: &[ActionInstance<Actions>]) -> Vec<ActionInstance<Actions>> {
        let mut result = vec![Connect { client_type: *self }.into()];
        result.extend_from_slice(actions);
        result
    }
}

fn start_node() -> Node {
    workers::block_on(Node::builder().build())
        .expect("run builder")
        .expect("start node")
}
