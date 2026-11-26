//! Who may push blocks into this node and who may read blocks from it.
//!
//! - [`AccessMode`]: whether access control is on (`Controlled`) or off
//!   (`Open`).
//! - [`ReplicatorRegistry`]: the peers that replicate each collection.
//! - The read gate ([`BlockReadGate`], [`BlockClassifier`], [`ServeAcp`]):
//!   the checks a block passes before the CAR serve path hands it to a peer.

mod mode;
mod read_gate;
mod registry;

pub use mode::AccessMode;
pub use read_gate::{
    AllowAllBlockReadGate, BlockAcpMeta, BlockClass, BlockClassifier, BlockReadGate,
    DefaultBlockClassifier, LateBoundServeAcp, ServeAcp,
};
pub use registry::ReplicatorRegistry;
