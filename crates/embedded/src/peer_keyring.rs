use std::{fmt, sync::Arc};

/// Application-owned storage for this node's peer identity.
///
/// Use a separate keyring namespace per node. This protects the transport key,
/// not signing identities or document encryption keys. Existing plaintext peer
/// keys require explicit migration before enabling this option, even if they
/// match the keyring. No migration helper is provided here.
///
/// Startup records the public identity in the database and requires the same
/// keyring on subsequent starts. Older binaries do not enforce that marker;
/// do not reopen a keyring-backed database with them.
/// Custom backends must return [`keyring::Error::NotFound`] only for missing
/// keys, not for a locked backend or a failed read.
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
