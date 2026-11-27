//! One pinc peer, assembled as the iroh peer is: endpoint, sync coordinator,
//! replication, event dispatch, retry, restore, and the management adapter.
//!
//! A copy of `iroh_peer` over `p2p::pinc::PincTransport`, kept apart so the
//! iroh assembly is untouched while pinc is evaluated. Native only: pinc has
//! no browser host yet.

mod config;
mod events;
mod peer;
mod shutdown;

pub use config::PincPeerConfig;
pub use peer::{
    PincBlockstore, PincCoordinator, PincManageChannel, PincPeer, PincReplicationStack,
};
pub use shutdown::PincPeerShutdown;
