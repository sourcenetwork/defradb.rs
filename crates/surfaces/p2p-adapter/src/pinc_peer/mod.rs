//! One pinc peer: endpoint, sync coordinator, replication, event dispatch,
//! retry, restore, and the management adapter. Native only: pinc has no
//! browser host yet (#3412).

mod config;
mod events;
mod peer;
mod shutdown;

pub use config::PincPeerConfig;
pub use peer::{
    PincBlockstore, PincCoordinator, PincManageChannel, PincPeer, PincReplicationStack,
};
pub use shutdown::PincPeerShutdown;
