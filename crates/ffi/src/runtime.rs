//! The C library's network runtime.
//!
//! Network calls (P2P, starting and closing a node, remote ACP) run on it,
//! blocking their C caller's thread ([`Edge::block_on`]); a node's data-path
//! calls run on its worker pool instead ([`crate::call`]). The first call
//! that needs it starts it, and it lives as long as the process.
//!
//! Starting it is also where the process is set up once: under Go's
//! integration tests, the library switches to Go's fixed test nonce and
//! encryption key, as `defra_init` did before it was removed.

use edge::{Edge, EdgeConfig, EdgeRuntime};
use regolith::sync::OnceCell;

static RUNTIME: OnceCell<EdgeRuntime> = OnceCell::new();

/// Why the runtime could not start.
static RUNTIME_ERROR: OnceCell<String> = OnceCell::new();

/// Lets test crypto run at all; release builds refuse it without this.
const DETERMINISTIC_TEST_CRYPTO_ENV: &str = "DEFRA_ALLOW_DETERMINISTIC_TEST_CRYPTO";
/// Set by Go's rustffi test client.
const RUST_FFI_CLIENT_ENV: &str = "DEFRA_CLIENT_RUST_FFI";

/// Under Go's integration tests, uses Go's fixed test nonce and encryption
/// key, so encrypted blocks and their CIDs match the values Go's tests
/// expect. Runs once per process, before the runtime starts: every node
/// starts the runtime before it can encrypt anything. Never on in
/// production: it needs `DEFRA_ALLOW_DETERMINISTIC_TEST_CRYPTO=1` and a
/// process that is Go's test suite.
fn init_process() {
    static INIT: OnceCell<()> = OnceCell::new();
    // Racing callers may each run this; it only switches flags on, so every
    // caller returns with them on, as with a `Once`.
    INIT.get_or_init_racy(|| {
        if should_use_deterministic_test_crypto_for_process() {
            crypto::encryption::nonce::set_deterministic_nonce(true);
            defra_core::encryption::set_deterministic_encryption_key(true);
        }
    });
}

fn deterministic_test_crypto_env_enabled() -> bool {
    matches!(
        std::env::var(DETERMINISTIC_TEST_CRYPTO_ENV).as_deref(),
        Ok("1")
    )
}

fn rust_ffi_client_env_enabled() -> bool {
    matches!(std::env::var(RUST_FFI_CLIENT_ENV).as_deref(), Ok("true"))
}

fn should_use_deterministic_test_crypto_for_process() -> bool {
    let arg0 = std::env::args().next();
    should_use_deterministic_test_crypto_for_process_state(
        deterministic_test_crypto_env_enabled(),
        rust_ffi_client_env_enabled(),
        arg0.as_deref(),
    )
}

/// Test crypto is on when it is allowed and the process is Go's test suite:
/// the rustffi client says so, or the binary is a Go test binary.
fn should_use_deterministic_test_crypto_for_process_state(
    env_enabled: bool,
    rust_ffi_client_enabled: bool,
    arg0: Option<&str>,
) -> bool {
    env_enabled
        && (rust_ffi_client_enabled
            || arg0.is_some_and(|arg0| should_use_deterministic_test_crypto(env_enabled, arg0)))
}

/// Whether `arg0` names a Go test binary (`go test`'s `*.test`, a test under
/// `defradb/tests/`, or a delve debug binary).
fn should_use_deterministic_test_crypto(env_enabled: bool, arg0: &str) -> bool {
    env_enabled
        && (arg0.ends_with(".test")
            || arg0.contains("/defradb/tests/")
            || arg0.contains("/__debug_bin"))
}

/// Start the network runtime.
///
/// Safe to call multiple times - only the first successful call starts it.
/// Returns true on success, false on failure.
/// On failure, call `runtime_init_error()` to get the error message.
pub fn init_runtime() -> bool {
    init_process();
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

    #[test]
    fn starting_the_runtime_outside_go_tests_keeps_random_keys() {
        if deterministic_test_crypto_env_enabled() {
            return;
        }
        assert!(init_runtime());
        assert!(
            !defra_core::encryption::deterministic_encryption_key_enabled(),
            "only Go's test suite gets fixed encryption keys"
        );
    }

    #[test]
    fn test_go_test_binary_detection_for_deterministic_test_crypto() {
        assert!(
            !should_use_deterministic_test_crypto(false, "/tmp/go-build123/tests.test"),
            "release FFI test crypto requires the explicit environment gate"
        );

        assert!(should_use_deterministic_test_crypto(
            true,
            "/tmp/go-build123/tests.test"
        ));
        assert!(should_use_deterministic_test_crypto(
            true,
            "/Users/me/defradb/tests/integration.test"
        ));
        assert!(should_use_deterministic_test_crypto(
            true,
            "/private/tmp/__debug_bin123"
        ));
        assert!(!should_use_deterministic_test_crypto(
            true,
            "/usr/local/bin/defradb"
        ));
    }

    #[test]
    fn test_ffi_test_env_detection_for_deterministic_test_crypto() {
        assert!(!should_use_deterministic_test_crypto_for_process_state(
            false,
            true,
            Some("/usr/local/bin/defradb")
        ));
        assert!(should_use_deterministic_test_crypto_for_process_state(
            true,
            true,
            Some("/usr/local/bin/defradb")
        ));
        assert!(!should_use_deterministic_test_crypto_for_process_state(
            true,
            false,
            Some("/usr/local/bin/defradb")
        ));
        assert!(should_use_deterministic_test_crypto_for_process_state(
            true,
            false,
            Some("/tmp/go-build123/tests.test")
        ));
    }
}
