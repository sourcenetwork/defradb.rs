use std::sync::Arc;

use defra_node::EmbeddedNode;
use testo::prelude::*;

use crate::actions::Actions;

/// What every action sees: the in-process node it acts on.
#[derive(Clone)]
pub struct State {
    pub node: Arc<EmbeddedNode>,
}

/// A Go-style integration test: a list of actions run in order against one
/// fresh in-process node.
#[derive(Default)]
pub struct TestCase {
    pub actions: Vec<ActionInstance<Actions>>,
}

impl TestCase {
    pub fn execute(self) {
        let node = workers::block_on(EmbeddedNode::builder().build())
            .expect("run builder")
            .expect("start node");
        testo::action::execute(
            State {
                node: Arc::new(node),
            },
            &self.actions,
        );
    }
}
