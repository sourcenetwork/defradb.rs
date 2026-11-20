//! Nodes, transactions and the library's version.

use std::ffi::c_int;

use super::{options, zero_default};
use crate::types::{DefraNewNodeResult, DefraNewTxnResult, DefraResult, NodeInitOptions};
use crate::{node, txn, version};

/// Options of `defra_transaction_create`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraTransactionOptions {
    /// A transaction that only reads (1=true, 0=false).
    pub read_only: c_int,
    /// Reserved, so the struct has no padding: leave it zero. It never
    /// becomes a field; new fields go after the struct's last one.
    pub reserved: u32,
}

/// Options of `defra_version_get`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraVersionOptions {
    /// Include build details (1=true, 0=false).
    pub full: c_int,
    /// Answer as JSON (1=true, 0=false).
    pub json: c_int,
}

zero_default!(
    NodeInitOptions,
    DefraTransactionOptions,
    DefraVersionOptions,
);

/// Starts a node. `defra_node_new` in `defra.h` passes the size.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes, whose strings
/// and byte buffers are valid as `NodeInitOptions` describes.
#[no_mangle]
pub unsafe extern "C" fn defra_node_new_sized(
    opts: *const NodeInitOptions,
    opts_size: u32,
) -> DefraNewNodeResult {
    let opts = options!(opts, opts_size, DefraNewNodeResult::error);
    node::defra_node_new(opts)
}

/// Creates a transaction on node `node`. Its handle stands in for the node
/// in any call that takes a target.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_transaction_create_sized(
    node: usize,
    opts: *const DefraTransactionOptions,
    opts_size: u32,
) -> DefraNewTxnResult {
    let opts = options!(opts, opts_size, DefraNewTxnResult::error);
    txn::defra_transaction_create(node, opts.read_only)
}

/// Answers the library's version.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_version_get_sized(
    opts: *const DefraVersionOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    version::defra_version_get(opts.full, opts.json)
}
