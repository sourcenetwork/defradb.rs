//! The C library's network runtime.
//!
//! Network calls (P2P, starting and closing a node, remote ACP) run on it,
//! blocking their C caller's thread ([`Edge::block_on`]); a node's data-path
//! calls run on its worker pool instead ([`crate::call`]). The first call
//! that needs it starts it, and it lives as long as the process.

use edge::{Edge, EdgeConfig, EdgeRuntime};
use regolith::sync::OnceCell;

static RUNTIME: OnceCell<EdgeRuntime> = OnceCell::new();

/// Why the runtime could not start.
static RUNTIME_ERROR: OnceCell<String> = OnceCell::new();

/// Start the network runtime.
///
/// Safe to call multiple times - only the first successful call starts it.
/// Returns true on success, false on failure.
/// On failure, call `runtime_init_error()` to get the error message.
pub fn init_runtime() -> bool {
    if RUNTIME.get().is_some() {
        return true;
    }
    if RUNTIME_ERROR.get().is_some() {
        return false;
    }
    match EdgeRuntime::start(EdgeConfig::default()) {
        Ok(runtime) => {
            // A racing call that published first wins; this runtime is
            // dropped, which stops it.
            let _ = RUNTIME.set(runtime);
            true
        }
        Err(error) => {
            let _ = RUNTIME_ERROR.set(error.to_string());
            false
        }
    }
}

/// Get the runtime initialization error, if any.
pub fn runtime_init_error() -> Option<&'static str> {
    RUNTIME_ERROR.get().map(String::as_str)
}

/// The network runtime's handle, starting the runtime on first use; `None`
/// when it could not start ([`runtime_init_error`] says why).
pub fn edge() -> Option<&'static Edge> {
    init_runtime();
    RUNTIME.get().map(EdgeRuntime::edge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_init() {
        assert!(init_runtime(), "runtime initialization should succeed");
        assert!(edge().is_some());
    }

    #[test]
    fn test_runtime_handle() {
        assert!(init_runtime(), "runtime initialization should succeed");
        let edge = edge().expect("the runtime started");

        // Can run async work, blocking this plain thread.
        assert_eq!(edge.block_on(async { 42 }), Ok(42));
    }

    #[test]
    fn test_runtime_init_idempotent() {
        // Multiple calls should all succeed, on one runtime.
        assert!(init_runtime());
        let first = edge().expect("the runtime started").clone();
        assert!(init_runtime());
        assert!(init_runtime());
        assert_eq!(edge(), Some(&first));
    }
}
