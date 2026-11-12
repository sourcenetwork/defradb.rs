//! One vinc peer, assembled as the iroh peer is: endpoint, sync coordinator,
//! replication, event dispatch, retry, restore, and the management adapter.
//!
//! A copy of `iroh_peer` over `p2p::vinc::VincTransport`, kept apart so the
//! iroh assembly is untouched while vinc is evaluated. Native only: vinc has
//! no browser host yet.

mod config;
mod events;
mod peer;
mod shutdown;

pub use config::VincPeerConfig;
pub use peer::{
    VincBlockstore, VincCoordinator, VincManageChannel, VincPeer, VincReplicationStack,
};
pub use shutdown::VincPeerShutdown;
