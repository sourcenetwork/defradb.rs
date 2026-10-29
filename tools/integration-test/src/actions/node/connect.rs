use serde::{Deserialize, Serialize};
use testo::prelude::*;

use crate::client_type::ClientType;
use crate::nodes::NodeSpec;
use crate::scenario::DEFAULT_NODE;
use crate::test_case::State;

/// Points the test at a fresh default node reached through `client_type`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connect {
    pub client_type: ClientType,
}

impl Action for Connect {
    type TState = State;

    fn execute(&self, state: &mut State) {
        state.client_type = Some(self.client_type);
        state.start_node(DEFAULT_NODE, NodeSpec::default());
    }
}
