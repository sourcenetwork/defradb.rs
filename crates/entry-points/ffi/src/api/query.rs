//! GraphQL requests and event subscriptions.

use std::ffi::c_char;

use super::{options, zero_default};
use crate::call::{DefraResultCallback, Reply};
use crate::types::DefraResult;
use crate::{query, subscription};

/// Options of `defra_query_execute` and `defra_query_execute_async`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraQueryOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
    /// The operation to run, for a request with several (null or empty for
    /// the only one).
    pub operation_name: *const c_char,
    /// The request's variables as a JSON object (null or empty for none).
    pub variables: *const c_char,
}

/// Options of `defra_event_subscribe`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraEventSubscribeOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

zero_default!(DefraQueryOptions, DefraEventSubscribeOptions);

/// Runs the GraphQL request `query` and answers with its JSON response.
///
/// # Safety
///
/// `query` must be null or a valid null-terminated UTF-8 string; `opts` must
/// be null or point to `opts_size` readable bytes whose strings are valid.
#[no_mangle]
pub unsafe extern "C" fn defra_query_execute_sized(
    target: usize,
    query: *const c_char,
    opts: *const DefraQueryOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    query::defra_query_execute(
        target,
        query,
        opts.identity,
        opts.operation_name,
        opts.variables,
    )
}

/// `defra_query_execute` for a caller called back instead of waiting:
/// `callback` receives `context` and the result exactly once. Options it
/// cannot read are reported to `callback` before this returns.
///
/// # Safety
///
/// As [`defra_query_execute_sized`]; the strings are read only before this
/// returns.
#[no_mangle]
pub unsafe extern "C" fn defra_query_execute_async_sized(
    target: usize,
    query: *const c_char,
    callback: DefraResultCallback,
    context: usize,
    opts: *const DefraQueryOptions,
    opts_size: u32,
) {
    let opts = match crate::options::read(opts, opts_size) {
        Ok(opts) => opts,
        Err(message) => return Reply::new(callback, context).send(DefraResult::error(message)),
    };
    query::defra_query_execute_async(
        target,
        query,
        opts.identity,
        opts.operation_name,
        opts.variables,
        callback,
        context,
    )
}

/// Subscribes to the node events named in `names`, comma-separated, and
/// answers with the subscription's ID for `defra_subscription_poll`.
///
/// # Safety
///
/// `names` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_event_subscribe_sized(
    target: usize,
    names: *const c_char,
    opts: *const DefraEventSubscribeOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    subscription::defra_event_subscribe(target, names, opts.identity)
}
