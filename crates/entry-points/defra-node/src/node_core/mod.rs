//! The node's operations, each taking the whole request context a scope
//! curries: [`Node`], [`crate::ScopedNode`] and [`crate::TxnNode`] build that
//! context and call in here, so the operations themselves are written once.

mod backup;
mod block;
mod collection;
mod dac;
mod dac_relationship;
mod dac_transfer;
mod document;
mod encrypted_index;
mod execute;
mod index;
mod nac;
mod observe;
mod owed;
#[cfg(feature = "managed-p2p")]
mod peer;
mod shared_transport;
mod transaction;
mod view;

pub use index::IndexListing;
pub(crate) use transaction::Begun;

use crate::Node;

/// The node's operations over a borrowed node.
pub(crate) struct NodeCore<'n> {
    node: &'n Node,
}

impl Node {
    /// The operations every scope of this node calls.
    pub(crate) fn core(&self) -> NodeCore<'_> {
        NodeCore { node: self }
    }
}
