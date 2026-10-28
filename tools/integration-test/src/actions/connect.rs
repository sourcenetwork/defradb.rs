use serde::{Deserialize, Serialize};
use testo::prelude::*;

use crate::client_type::ClientType;
use crate::test_case::State;

/// Points the test at a fresh database reached through `client_type`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connect {
    pub client_type: ClientType,
}

impl Action for Connect {
    type TState = State;

    fn execute(&self, state: &mut State) {
        let connection = self.client_type.connect();
        state.client = Some(connection.client);
        state.held = connection.held;
    }
}
