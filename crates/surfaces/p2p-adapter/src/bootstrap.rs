//! The dials of a node's bootstrap peers (`net.peers`,
//! `PincConfig::bootstrap_peers`).

use std::sync::Arc;

use crate::P2POperations;

/// The dials of `peers`, for the node to run once its access control is
/// wired. Each is dialed as the node ([`P2POperations::connect_bootstrap_peer`]),
/// so node access control does not refuse it, in the background on `edge`,
/// so one that does not answer does not hold up the start. A dial that fails
/// is a warning.
pub fn bootstrap_dials(
    ops: Arc<dyn P2POperations>,
    edge: edge::Edge,
    peers: Vec<String>,
) -> Box<dyn FnOnce() + Send> {
    Box::new(move || {
        for addr in peers {
            let ops = Arc::clone(&ops);
            edge.spawn_service(async move {
                if let Err(error) = ops.connect_bootstrap_peer(&addr).await {
                    tracing::warn!(peer = %addr, %error, "bootstrap peer did not connect");
                }
            })
            .detach();
        }
    })
}
