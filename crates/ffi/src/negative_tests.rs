//! Negative and boundary tests for FFI security (findings 07-51, 07-52).
//!
//! Tests: NULL pointers, invalid handles, non-UTF-8 strings, handle lifecycle
//! stress, and concurrent registry access.

use std::ffi::c_char;
use std::ptr;
use std::sync::Arc;
use std::thread;

use regolith::sync::Barrier;

use crate::collection::defra_collection_add;
use crate::identities::{defra_identity_free, defra_identity_new};
use crate::node::{defra_node_close, defra_node_new};
use crate::query::defra_query_execute;
use crate::subscription::{defra_subscription_close, defra_subscription_poll};
use crate::test_support::{close, cstr, err, node, ok, take};
use crate::txn::{defra_transaction_commit, defra_transaction_create, defra_transaction_discard};
use crate::types::{defra_new_txn_result_free, defra_result_free, DefraResult};

// ── NULL pointer handling (07-51) ─────────────────────────────────────────

/// A NULL SDL reads as Go's empty string: an SDL with no collection, not a
/// crash.
#[test]
fn add_collection_null_sdl_is_empty() {
    let node = node();
    let (status, _) = take(unsafe { defra_collection_add(node, ptr::null(), 0) });
    assert!(
        status == 0 || status == 1,
        "status must be 0 or 1, got {status}"
    );
    close(node);
}

/// A NULL query reads as Go's empty string and fails as GraphQL, not a crash.
#[test]
fn execute_query_null_query_does_not_crash() {
    let node = node();
    let (status, _) =
        take(unsafe { defra_query_execute(node, ptr::null(), 0, ptr::null(), ptr::null()) });
    assert!(
        status == 0 || status == 1,
        "status must be 0 or 1, got {status}"
    );
    close(node);
}

/// defra_result_free on a NULL pointer must not crash.
#[test]
fn free_result_null_is_safe() {
    unsafe { defra_result_free(ptr::null_mut()) };
}

// ── Invalid handle handling (07-51) ───────────────────────────────────────

/// The zero handle is always invalid.
#[test]
fn node_close_zero_handle_returns_error() {
    let error = err(defra_node_close(0));
    assert!(
        error.contains("invalid"),
        "should say invalid, got: {error}"
    );
}

/// A large handle that was never issued is invalid.
#[test]
fn node_close_large_handle_returns_error() {
    err(defra_node_close(usize::MAX));
}

/// An identity handle is not a node handle.
#[test]
fn an_identity_handle_is_not_a_node() {
    let identity = unsafe { defra_identity_new(ptr::null()) };
    assert_eq!(identity.status, 0);
    err(defra_node_close(identity.identity_ptr));
    defra_identity_free(identity.identity_ptr);
}

/// A node handle is not an identity handle.
#[test]
fn a_node_handle_is_not_an_identity() {
    let node = node();
    let request = cstr("{ __typename }");
    let error =
        err(unsafe { defra_query_execute(node, request.as_ptr(), node, ptr::null(), ptr::null()) });
    assert!(error.contains("invalid handle"), "got: {error}");
    close(node);
}

/// Stale handle: close a node then attempt operations using the old handle.
#[test]
fn stale_handle_operations_return_errors() {
    let stale = node();
    close(stale);

    err(defra_node_close(stale));

    let sdl = cstr("type X { v: Int }");
    err(unsafe { defra_collection_add(stale, sdl.as_ptr(), 0) });

    let request = cstr("{ X { v } }");
    err(unsafe { defra_query_execute(stale, request.as_ptr(), 0, ptr::null(), ptr::null()) });

    let mut txn = defra_transaction_create(stale, 0);
    assert_eq!(
        txn.status, 1,
        "creating a transaction on a stale handle must fail"
    );
    unsafe { defra_new_txn_result_free(&mut txn) };
}

/// A committed transaction's handle is released.
#[test]
fn a_committed_transaction_handle_is_stale() {
    let node = node();
    let txn = defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0);
    ok(defra_transaction_commit(txn.txn_ptr));
    err(defra_transaction_commit(txn.txn_ptr));
    // Discarding a released handle does nothing.
    defra_transaction_discard(txn.txn_ptr);
    close(node);
}

/// Unknown subscription IDs must not crash poll or close.
#[test]
fn invalid_subscription_ids_return_errors() {
    for id in ["0", "not-a-number", ""] {
        let id = cstr(id);
        err(unsafe { defra_subscription_poll(id.as_ptr()) });
        // Closing an unknown ID does nothing, as in Go.
        ok(unsafe { defra_subscription_close(id.as_ptr()) });
    }
    err(unsafe { defra_subscription_poll(ptr::null()) });
}

// ── Non-UTF-8 string handling (07-51) ─────────────────────────────────────

/// A C string's invalid UTF-8 reads lossily (U+FFFD for each bad sequence):
/// the call fails or succeeds on that text, and never crashes.
#[test]
fn add_collection_invalid_utf8_sdl_does_not_crash() {
    let node = node();
    let invalid_utf8: &[u8] = &[0xFF, 0xFE, 0xFD, 0x00];
    let sdl = invalid_utf8.as_ptr() as *const c_char;
    let (status, _) = take(unsafe { defra_collection_add(node, sdl, 0) });
    assert!(
        status == 0 || status == 1,
        "status must be 0 or 1, got {status}"
    );
    close(node);
}

/// A query of invalid UTF-8 must not crash.
#[test]
fn execute_query_invalid_utf8_query_does_not_crash() {
    let node = node();
    let invalid_utf8: &[u8] = &[0xC0, 0xAF, 0x00];
    let request = invalid_utf8.as_ptr() as *const c_char;
    let (status, _) =
        take(unsafe { defra_query_execute(node, request, 0, ptr::null(), ptr::null()) });
    assert!(
        status == 0 || status == 1,
        "must not crash, status={status}"
    );
    close(node);
}

// ── Handle lifecycle stress (07-52) ───────────────────────────────────────

/// Rapid sequential create-and-destroy produces unique handles and never
/// panics or leaks.
#[test]
fn handle_lifecycle_rapid_sequential_create_destroy() {
    let handles: Vec<usize> = (0..50).map(|_| node()).collect();

    let unique: rapidhash::RapidHashSet<usize> = handles.iter().copied().collect();
    assert_eq!(unique.len(), handles.len(), "all handles must be unique");

    for handle in handles.into_iter().rev() {
        close(handle);
    }
}

/// Node, transaction and identity handles come from one counter, so no two
/// live objects share a handle.
#[test]
fn node_transaction_and_identity_handles_never_collide() {
    let node = node();
    let txn = defra_transaction_create(node, 1);
    assert_eq!(txn.status, 0);
    let identity = unsafe { defra_identity_new(ptr::null()) };
    assert_eq!(identity.status, 0);

    let handles = [node, txn.txn_ptr, identity.identity_ptr];
    let unique: rapidhash::RapidHashSet<usize> = handles.iter().copied().collect();
    assert_eq!(
        unique.len(),
        handles.len(),
        "handles must be unique across kinds"
    );

    defra_transaction_discard(txn.txn_ptr);
    defra_identity_free(identity.identity_ptr);
    close(node);
}

/// Concurrent node creation and destruction from multiple threads must not
/// corrupt the registry or produce duplicate handles.
#[test]
fn concurrent_node_create_destroy_is_safe() {
    const THREADS: usize = 4;
    const NODES_PER_THREAD: usize = 10;

    let barrier = Arc::new(Barrier::new(THREADS));
    let workers: Vec<_> = (0..THREADS)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                // All threads start together to maximize contention.
                crate::runtime::edge()
                    .expect("runtime started")
                    .block_on(barrier.wait())
                    .expect("the barrier wait runs on the runtime");

                let handles: Vec<usize> = (0..NODES_PER_THREAD).map(|_| node()).collect();
                for handle in handles {
                    close(handle);
                }
            })
        })
        .collect();

    for worker in workers {
        worker.join().expect("thread must not panic");
    }
}

// ── Panic containment at the FFI boundary (#1594) ─────────────────────────

/// An internal panic must surface as an error result, not kill the process,
/// and must leave the process able to serve the next request.
#[test]
fn ffi_entry_converts_panic_to_error_result() {
    let result: DefraResult = crate::ffi_entry! {
        panic!("deliberate test panic")
    };
    let error = err(result);
    assert!(
        error.contains("internal error (panic)") && error.contains("deliberate test panic"),
        "error should carry the panic message, got: {error}"
    );

    // A live round-trip after the caught panic: the process is still usable.
    let node = node();
    crate::test_support::add_collection(node, "type Ping { v: Int }");
    crate::test_support::query(node, "query { Ping { v } }");
    close(node);
}

/// The release profile must not set `panic = "abort"`.
///
/// Unit tests build with the dev profile, where `panic = "unwind"` is
/// already the default — so no runtime test can catch a regression that
/// reintroduces `panic = "abort"` and silently turns the `ffi_entry!`
/// `catch_unwind` into a no-op in the shipped dylib. Only this config
/// assertion can (#1594).
#[test]
fn release_profile_does_not_abort_on_panic() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"))
            .expect("workspace Cargo.toml must be readable")
            .replace("\r\n", "\n");

    let after_header = manifest
        .split_once("[profile.release]\n")
        .expect("workspace must define [profile.release]")
        .1;
    let section = after_header
        .split_once("\n[")
        .map_or(after_header, |(section, _)| section);

    let offender = section.lines().map(str::trim).find(|line| {
        line.split_once('=').is_some_and(|(key, value)| {
            key.trim() == "panic" && value.trim().trim_matches('"') != "unwind"
        })
    });

    assert_eq!(
        offender, None,
        "[profile.release] must leave panics unwinding, got:\n{section}"
    );
}

/// The node options a zero-initialized C struct gives start the node Go's
/// would: a node a C caller builds with `NodeInitOptions opts = {0}` must
/// not need any other field set, here with P2P off so the test binds nothing.
#[test]
fn a_node_starts_from_the_default_options() {
    let handle = unsafe {
        defra_node_new(crate::types::NodeInitOptions {
            disable_p2p: 1,
            ..Default::default()
        })
    };
    assert_eq!(handle.status, 0);
    close(handle.node_ptr);
}
