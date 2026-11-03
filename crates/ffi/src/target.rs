//! What a call's handles name.
//!
//! Like Go's cbindings, a call that works on a store takes one handle that
//! names either a node or one of its open transactions, and an identity
//! handle that names the caller (0 for none). Handles come from one counter
//! (`state::handle`), so a handle is in at most one registry.

use std::ffi::c_char;
use std::sync::Arc;

use crate::state::{TxnState, IDENTITIES, NODES, TXNS};
use crate::types::{c_str_to_string, CollectionOptions, DefraResult};

/// What a call made with a handle the registries do not hold reports.
pub const ERR_INVALID_HANDLE: &str = "invalid handle";

/// The store a call works on.
#[derive(Clone)]
pub(crate) enum Target {
    /// A node, each operation in its own transaction.
    Node(usize),
    /// An open transaction of a node.
    Txn(Arc<TxnState>),
}

impl Target {
    /// The node the call runs on.
    pub(crate) fn node(&self) -> usize {
        match self {
            Target::Node(node) => *node,
            Target::Txn(txn) => txn.node,
        }
    }

    /// The transaction the call runs in, if it runs in one.
    pub(crate) fn txn_id(&self) -> Option<defra_core::TxnId> {
        match self {
            Target::Node(_) => None,
            Target::Txn(txn) => Some(txn.id),
        }
    }
}

/// The node or transaction `handle` names.
pub(crate) fn resolve_target(handle: usize) -> Result<Target, DefraResult> {
    if NODES.contains(handle) {
        return Ok(Target::Node(handle));
    }
    TXNS.get(handle)
        .map(Target::Txn)
        .ok_or_else(|| DefraResult::error(ERR_INVALID_HANDLE))
}

/// The node `handle` names, for a call that only works on a node.
pub(crate) fn resolve_node(handle: usize) -> Result<usize, DefraResult> {
    if NODES.contains(handle) {
        Ok(handle)
    } else {
        Err(DefraResult::error(ERR_INVALID_HANDLE))
    }
}

/// The DID of the identity `handle` names; `None` for the null handle.
pub(crate) fn resolve_identity(handle: usize) -> Result<Option<String>, DefraResult> {
    if handle == 0 {
        return Ok(None);
    }
    IDENTITIES
        .get(handle)
        .map(|identity| Some(identity.did.clone()))
        .ok_or_else(|| DefraResult::error(ERR_INVALID_HANDLE))
}

/// A call's [`CollectionOptions`], read from the caller's strings.
#[derive(Debug, Clone, Default)]
pub(crate) struct CollectionArgs {
    pub(crate) version: Option<String>,
    pub(crate) collection_id: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) get_inactive: bool,
    /// `None` for the node default.
    pub(crate) signing: Option<bool>,
}

impl CollectionArgs {
    /// # Safety
    ///
    /// Every non-null string in `options` must be a valid null-terminated
    /// string.
    pub(crate) unsafe fn read(options: &CollectionOptions) -> Self {
        Self {
            version: non_empty(options.version),
            collection_id: non_empty(options.collection_id),
            name: non_empty(options.name),
            get_inactive: options.get_inactive != 0,
            signing: match options.enable_signing {
                0 => None,
                value => Some(value > 0),
            },
        }
    }

    /// The collection lookup these options select, as Go's
    /// `GetCollectionsOptions` do.
    pub(crate) fn selector(&self) -> db::CollectionSelector {
        db::CollectionSelector {
            names: self.name.clone().map(|name| vec![name]),
            version_id: self.version.clone(),
            collection_id: self.collection_id.clone(),
            get_inactive: self.get_inactive,
        }
    }
}

/// A C string as Go's `C.GoString` reads it, with empty meaning absent.
///
/// # Safety
///
/// `ptr` must be null or a valid null-terminated string.
pub(crate) unsafe fn non_empty(ptr: *const c_char) -> Option<String> {
    c_str_to_string(ptr).filter(|value| !value.is_empty())
}

/// A C string as Go's `C.GoString` reads it: null is empty.
///
/// # Safety
///
/// `ptr` must be null or a valid null-terminated string.
pub(crate) unsafe fn go_string(ptr: *const c_char) -> String {
    c_str_to_string(ptr).unwrap_or_default()
}

/// A comma-separated list as Go's `splitCommaSeparatedString` splits it.
pub(crate) fn split_comma_separated(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split(',').map(str::to_string).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::split_comma_separated;

    #[test]
    fn a_comma_separated_list_splits_on_every_comma() {
        assert!(split_comma_separated("").is_empty());
        assert_eq!(split_comma_separated("User"), ["User"]);
        assert_eq!(split_comma_separated("User,Post"), ["User", "Post"]);
        assert_eq!(split_comma_separated("User,,Post"), ["User", "", "Post"]);
    }
}
