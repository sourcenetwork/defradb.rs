//! DefraDB's C API.
//!
//! The exports cover the calls of Go DefraDB's `cbindings`
//! (`libdefradb.h`), each named `defra_<noun>_<verb>` after the Go file that
//! defines it (`collection_add.go` is `defra_collection_add`). The API, not
//! the ABI: the names and types are prefixed (`DefraResult`, not Go's
//! `Result`), and [`types::NodeInitOptions`] adds fields after Go's, so a
//! program built against Go's header does not link or run against this
//! library unchanged. Like Go's, a call that
//! works on a store takes one handle naming a node or one of its open
//! transactions, and an identity handle naming the caller (0 for none); it
//! answers with a [`types::DefraResult`] the caller frees with
//! `defra_result_free`.
//!
//! The C header is generated: `just build-ffi-header`.

pub mod acp;
pub mod action;
pub mod backup;
pub mod batch;
pub mod block;
pub mod call;
pub mod collection;
pub mod document;
pub mod dump;
mod duration;
pub mod encrypted_index;
pub mod helpers;
pub mod identities;
pub mod index;
pub mod lens;
pub mod nac_check;
pub mod node;
pub mod p2p;
#[cfg(feature = "profiling")]
pub mod profiling;
pub mod query;
pub mod remote_signer;
pub mod request;
pub mod runtime;
pub mod state;
pub mod subscription;
pub mod target;
pub mod txn;
pub mod types;
pub mod version;
pub mod view;

pub use helpers::{get_node_database, get_node_runner, get_rt, require_c_str};
pub use types::FfiPanicResult;

/// Error message for invalid node handle.
pub const ERR_INVALID_NODE_HANDLE: &str = "invalid node handle";

/// Extract a human-readable message from a caught panic payload.
pub fn extract_panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    let detail = panic
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("unknown panic");
    format!("internal error (panic): {}", detail)
}

/// Wrap an FFI function body in `catch_unwind` to prevent panics from
/// crossing the FFI boundary.
///
/// The return type must implement [`FfiPanicResult`]. Caught panics are
/// converted to error results via that trait.
///
/// The workspace release profile keeps the default `panic = "unwind"`
/// precisely so this wrapper works in shipped artifacts. Setting
/// `panic = "abort"` would kill the host process on any internal panic;
/// `release_profile_does_not_abort_on_panic` in `negative_tests.rs` guards
/// against that.
#[macro_export]
macro_rules! ffi_entry {
    ($($body:tt)*) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            $($body)*
        })) {
            Ok(result) => result,
            Err(panic) => {
                let msg = $crate::extract_panic_message(&panic);
                $crate::FfiPanicResult::from_panic(msg)
            }
        }
    };
}

/// Early-return on `Result<T, DefraResult>::Err`.
#[macro_export]
macro_rules! try_ffi {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => return e,
        }
    };
}

/// Run an async block on node `$node`'s worker pool, the C caller's thread
/// parked until it completes, and convert its `Result<String, String>` to
/// `DefraResult`. `caller` instead of a node runs the block on the caller's
/// thread: for work that routes itself into the pool (a GraphQL request, an
/// explicit transaction's call) or that only reads memory.
#[macro_export]
macro_rules! ffi_async {
    (caller, $body:block) => {
        $crate::ffi_async!(@into String, $crate::call::wait(async move $body))
    };
    (target $target:expr, $body:block) => {
        $crate::ffi_async!(@into String, $crate::call::on_target($target, move || async move $body))
    };
    ($node:expr, $body:block) => {
        $crate::ffi_async!(@into String, $crate::call::on_node($node, move || async move $body))
    };
    (@into $value:ty, $result:expr) => {{
        let result: Result<$value, String> = $result;
        $crate::types::DefraResult::from(result)
    }};
}

/// [`ffi_async!`] for void operations: `Result<(), String>` to `DefraResult`.
#[macro_export]
macro_rules! ffi_async_ok {
    (caller, $body:block) => {
        $crate::ffi_async!(@into (), $crate::call::wait(async move $body))
    };
    (target $target:expr, $body:block) => {
        $crate::ffi_async!(@into (), $crate::call::on_target($target, move || async move $body))
    };
    ($node:expr, $body:block) => {
        $crate::ffi_async!(@into (), $crate::call::on_node($node, move || async move $body))
    };
}

#[cfg(test)]
mod abi_tests;
#[cfg(test)]
mod api_refusal_tests;
#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod identities_tests;
#[cfg(test)]
mod identity_keys_tests;
#[cfg(test)]
mod negative_tests;
#[cfg(test)]
mod test_support;
