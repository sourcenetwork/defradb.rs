use serde::{Deserialize, Serialize};
use testo::action::ActionInstance;
use testo::prelude::*;

use super::Actions;
use crate::test_case::State;

/// Runs its actions at the same time. Each child expects its outcome as
/// `{"result": .., "error": ..}`, and a child that fails does not fail the
/// test.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Parallel {
    pub actions: Vec<Actions>,
}

impl Action for Parallel {
    type TState = State;

    fn execute(&self, state: &mut State) {
        let children: Vec<ActionInstance<Actions>> = self
            .actions
            .iter()
            .cloned()
            .map(ActionInstance::Async)
            .collect();
        drop(testo::action::execute(state.clone(), &children));
    }
}
