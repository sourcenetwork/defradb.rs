//! A transport's hold on its node's method table.

use std::sync::Arc;

use regolith::sync::Atom;
use vera_jsonrpc::LocalRpc;

/// The method table of one node, until that node's host lets go of it.
///
/// The table keeps the node's state alive, and with it the lock on the
/// node's directory. A provider can outlive the node it was built for, so
/// the host that stops the node closes the link: every transport sharing
/// it then refuses its calls, and the directory is free for the next node.
#[derive(Clone)]
pub(crate) struct Link(Arc<Atom<Option<LocalRpc>>>);

impl Link {
    pub(crate) fn new(rpc: LocalRpc) -> Self {
        Self(Arc::new(Atom::new(Some(rpc))))
    }

    /// The method table, unless the link is closed.
    pub(crate) fn rpc(&self) -> Option<LocalRpc> {
        self.0.load_clone()
    }

    #[cfg(feature = "node")]
    pub(crate) fn close(&self) {
        self.0.store(None);
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Link")
            .field("open", &self.rpc().is_some())
            .finish()
    }
}
