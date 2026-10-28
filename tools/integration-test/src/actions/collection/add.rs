use testo::prelude::*;

use crate::actions::expect::expect_error;
use crate::test_case::State;

/// Adds the collections declared in `sdl`.
#[derive(Clone, Debug, Default)]
pub struct AddCollection {
    pub sdl: &'static str,
    pub expected_error: Option<&'static str>,
}

impl Action for AddCollection {
    type TState = State;

    fn execute(&self, state: &mut State) {
        let result =
            workers::block_on(state.node.add_collection(self.sdl)).expect("run add_collection");
        expect_error(result, self.expected_error);
    }
}
