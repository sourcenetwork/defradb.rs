//! Access control mode for P2P synchronization.
//!
//! Access control is enforced on **both the ingress and egress paths**:
//!
//! - **Ingress:** `SyncCoordinator` checks incoming PushLog and topic
//!   messages against the `ReplicatorRegistry` before storing blocks.
//!   Unauthorized peers cannot push blocks into this node.
//! - **Egress:** the CAR serve path passes every block through the read gate
//!   (`crate::access::BlockReadGate`), which decodes the block's collection
//!   and consults the same `ReplicatorRegistry`. Peers that are not
//!   registered replicators for the block's collection are denied
//!   (signature and definition blocks are exempt to allow signature
//!   verification and schema bootstrap).
//!
//! This matches Go DefraDB's `bitswap.WithPeerBlockRequestFilter(hasAccess)`
//! wiring (`go-p2p/peer.go:146`), closing issue #830.
//!
//! In `AccessMode::Open` the gate allows every read, the behaviour of a node
//! that has not enabled ACP.

/// Access control mode for P2P synchronization.
///
/// Controls whether access control is enforced at the SyncCoordinator level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum AccessMode {
    /// No access control - all requests are allowed.
    /// This is the default mode when ACP is not configured.
    #[default]
    Open,

    /// Access control enabled - check replicator status.
    /// Only replicators for the specific collection have access.
    Controlled,
}

impl AccessMode {
    /// Returns true if access control is enabled.
    pub fn is_controlled(&self) -> bool {
        matches!(self, AccessMode::Controlled)
    }

    /// Returns true if access is open (no control).
    pub fn is_open(&self) -> bool {
        matches!(self, AccessMode::Open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_mode_helpers() {
        assert!(AccessMode::Open.is_open());
        assert!(!AccessMode::Open.is_controlled());
        assert!(AccessMode::Controlled.is_controlled());
        assert!(!AccessMode::Controlled.is_open());
        assert_eq!(AccessMode::default(), AccessMode::Open);
    }
}
