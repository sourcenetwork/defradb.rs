//! One iroh peer, assembled the same way for every node that runs one.
//!
//! `defra start`, the embedded and FFI node, `defra-node`, and the browser all
//! run the same stack: endpoint, sync coordinator, replication, event
//! dispatch, retry, restore, and the management adapter. What differs between
//! them is configuration, and what each does with the peer afterwards.

mod config;
mod events;
mod peer;
mod shutdown;

pub use config::IrohPeerConfig;
pub use peer::{IrohBlockstore, IrohCoordinator, IrohPeer, IrohReplicationStack, ManageChannel};
pub use shutdown::IrohPeerShutdown;
