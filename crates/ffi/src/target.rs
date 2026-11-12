//! What a call's handles name.
//!
//! Like Go's cbindings, a call that works on a store takes one handle that
//! names either a node or one of its open transactions, and an identity
//! handle that names the caller (0 for none). Handles come from one counter
//! (`state::handle`), so a handle is in at most one registry.

use std::ffi::c_char;
use std::sync::Arc;

use zeroize::Zeroizing;

use crate::state::{IdentityState, Ticket, TxnState, IDENTITIES, NODES, TXNS};
use crate::types::{c_str_to_string, CollectionOptions, DefraResult};

/// What a call made with a handle the registries do not hold reports.
pub const ERR_INVALID_HANDLE: &str = "invalid handle";

/// The store a call works on.
#[derive(Clone)]
pub(crate) enum Target {
    /// A node, each operation in its own transaction.
    Node(usize),
    /// An open transaction of a node, with the call's turn on it.
    Txn(Arc<TxnCall>),
}

/// One call on a transaction: the transaction, and the call's ticket, which
/// it holds until it ends, so calls on the transaction run in call order
/// (`proofs/tla/TxnTurns.tla`).
pub(crate) struct TxnCall {
    txn: Arc<TxnState>,
    ticket: Ticket,
}

impl TxnCall {
    /// Waits until it is this call's turn on its transaction.
    pub(crate) async fn turn(&self) {
        self.ticket.wait().await;
    }
}

impl Target {
    /// The node the call runs on.
    pub(crate) fn node(&self) -> usize {
        match self {
            Target::Node(node) => *node,
            Target::Txn(call) => call.txn.node,
        }
    }

    /// The transaction the call runs in, if it runs in one.
    pub(crate) fn txn_id(&self) -> Option<defra_core::TxnId> {
        match self {
            Target::Node(_) => None,
            Target::Txn(call) => Some(call.txn.id),
        }
    }

    /// Waits until it is the call's turn on its transaction; at once on a
    /// node.
    pub(crate) async fn turn(&self) {
        if let Target::Txn(call) = self {
            call.turn().await;
        }
    }
}

/// The node or transaction `handle` names. Naming a transaction takes the
/// call's ticket on it, in the order calls name it.
pub(crate) fn resolve_target(handle: usize) -> Result<Target, DefraResult> {
    if NODES.contains(handle) {
        return Ok(Target::Node(handle));
    }
    TXNS.get(handle)
        .map(|txn| {
            Target::Txn(Arc::new(TxnCall {
                ticket: txn.turns.take(),
                txn,
            }))
        })
        .ok_or_else(|| DefraResult::error(ERR_INVALID_HANDLE))
}

/// The node `handle` names, or the node of the transaction it names, for a
/// call that only needs the node: it takes no turn on the transaction.
pub(crate) fn resolve_target_node(handle: usize) -> Result<usize, DefraResult> {
    if NODES.contains(handle) {
        return Ok(handle);
    }
    TXNS.get(handle)
        .map(|txn| txn.node)
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
    resolve_identity_state(handle).map(|identity| identity.map(|identity| identity.did.clone()))
}

/// Who the identity `handle` names acts as: its DID, and the bearer token it
/// acts by when a token stands for it. Both `None` for the null handle.
pub(crate) fn resolve_caller(handle: usize) -> Result<Caller, DefraResult> {
    Ok(match resolve_identity_state(handle)? {
        Some(identity) => Caller {
            did: Some(identity.did.clone()),
            bearer: identity.bearer_token.clone(),
            _identity: Some(identity),
        },
        None => Caller::default(),
    })
}

/// The identity a call acts as.
#[derive(Clone, Default)]
pub(crate) struct Caller {
    /// Its DID; `None` for no identity.
    pub(crate) did: Option<String>,
    /// The bearer token it acts by, for an identity a token stands for.
    pub(crate) bearer: Option<Zeroizing<String>>,
    /// The identity itself, held while the call lives: its hold keeps the
    /// identity's key in the signing store, so a `defra_identity_free` on
    /// another thread cannot make the call sign with the node's key while it
    /// acts as this DID.
    _identity: Option<Arc<IdentityState>>,
}

impl Caller {
    /// `context` presenting this caller's bearer token, so a remote ACP
    /// provider acts with the token of the handle that made the call. A
    /// caller without one leaves `context` unchanged.
    pub(crate) fn present(
        &self,
        context: defra_core::RequestContext,
    ) -> defra_core::RequestContext {
        match &self.bearer {
            Some(token) => context.with_bearer(self.did.clone(), Some(token.to_string())),
            None => context,
        }
    }
}

/// The identity `handle` names; `None` for the null handle. Holding it keeps
/// the identity's key in the signing store, whatever happens to the handle.
pub(crate) fn resolve_identity_state(
    handle: usize,
) -> Result<Option<Arc<IdentityState>>, DefraResult> {
    if handle == 0 {
        return Ok(None);
    }
    IDENTITIES
        .get(handle)
        .map(Some)
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
