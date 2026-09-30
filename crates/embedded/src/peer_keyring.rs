use std::{fmt, sync::Arc};

/// Application-owned storage for this node's peer identity.
///
/// Use a separate keyring namespace per node. This protects the transport key,
/// not signing identities or document encryption keys. Existing plaintext peer
/// keys require explicit migration before enabling this option.
#[derive(Clone)]
pub struct PeerKeyring(pub(crate) Arc<dyn keyring::Keyring>);

impl PeerKeyring {
    pub fn new(keyring: Arc<dyn keyring::Keyring>) -> Self {
        Self(keyring)
    }
}

impl fmt::Debug for PeerKeyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PeerKeyring")
            .finish_non_exhaustive()
    }
}

impl PartialEq for PeerKeyring {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
