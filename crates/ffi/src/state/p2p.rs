use std::sync::Arc;

/// P2P state for FFI nodes.
pub struct P2PState {
    pub system: Arc<defra_node::ManagedP2PSystem>,
}

impl P2PState {
    pub fn new(system: Arc<defra_node::ManagedP2PSystem>) -> Self {
        Self { system }
    }
}
