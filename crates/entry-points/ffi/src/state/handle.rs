use std::sync::atomic::{AtomicUsize, Ordering};

/// The next handle given out, shared by nodes, transactions and identities so
/// that no two live objects ever share a handle: a call that takes "a node or
/// a transaction" tells them apart by which registry holds the handle.
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(1);

/// A fresh handle, never 0 (the null handle) and never one given out before.
pub(crate) fn next_handle() -> usize {
    NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)
}
