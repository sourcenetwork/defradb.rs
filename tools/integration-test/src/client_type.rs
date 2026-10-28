use std::any::Any;
use std::sync::Arc;

use client::Client;
use defra_node::EmbeddedNode;
use serde::{Deserialize, Serialize};
use testo::prelude::*;

use crate::actions::{Actions, Connect};

/// How a test reaches its database. The Go suite's `DEFRA_CLIENT_*` switches.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientType {
    /// Calls the embedded node directly.
    InProcess,
}

/// A client and what has to outlive it.
pub struct Connection {
    pub client: Arc<dyn Client>,
    pub held: Vec<Arc<dyn Any + Send + Sync>>,
}

impl ClientType {
    pub const ALL: [ClientType; 1] = [Self::InProcess];

    pub fn connect(self) -> Connection {
        match self {
            Self::InProcess => Connection {
                client: Arc::new(start_node()),
                held: Vec::new(),
            },
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

fn start_node() -> EmbeddedNode {
    workers::block_on(EmbeddedNode::builder().build())
        .expect("run builder")
        .expect("start node")
}
