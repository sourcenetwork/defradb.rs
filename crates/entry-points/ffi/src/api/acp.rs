//! Document and node access control, action listing, batch signing and
//! block signature checks.

use std::ffi::c_char;

use super::{options, zero_default};
use crate::types::DefraResult;
use crate::{acp, action, batch, block};

/// Options of `defra_acp_dac_policy_add`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraDacPolicyAddOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of `defra_acp_dac_relationship_add` and
/// `defra_acp_dac_relationship_delete`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraDacRelationshipOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of the node access control calls status, disable, re-enable and
/// purge.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraNacOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of `defra_acp_nac_relationship_add` and
/// `defra_acp_nac_relationship_delete`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraNacRelationshipOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of `defra_action_list`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraActionListOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of `defra_batch_start` and `defra_batch_sign`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraBatchOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of `defra_block_verify`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraBlockVerifyOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

zero_default!(
    DefraDacPolicyAddOptions,
    DefraDacRelationshipOptions,
    DefraNacOptions,
    DefraNacRelationshipOptions,
    DefraActionListOptions,
    DefraBatchOptions,
    DefraBlockVerifyOptions,
);

/// Adds a document access control policy, YAML or JSON.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string; `opts` must
/// be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_acp_dac_policy_add_sized(
    target: usize,
    policy: *const c_char,
    opts: *const DefraDacPolicyAddOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    acp::defra_acp_dac_policy_add(target, opts.identity, policy)
}

/// Gives `actor` `relation` on document `doc_id` of `collection`.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_acp_dac_relationship_add_sized(
    target: usize,
    collection: *const c_char,
    doc_id: *const c_char,
    relation: *const c_char,
    actor: *const c_char,
    opts: *const DefraDacRelationshipOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    acp::defra_acp_dac_relationship_add(target, opts.identity, collection, doc_id, relation, actor)
}

/// Takes `relation` on document `doc_id` of `collection` from `actor`.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_acp_dac_relationship_delete_sized(
    target: usize,
    collection: *const c_char,
    doc_id: *const c_char,
    relation: *const c_char,
    actor: *const c_char,
    opts: *const DefraDacRelationshipOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    acp::defra_acp_dac_relationship_delete(
        target,
        opts.identity,
        collection,
        doc_id,
        relation,
        actor,
    )
}

/// Answers node access control's status.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_acp_nac_status_sized(
    target: usize,
    opts: *const DefraNacOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    acp::defra_acp_nac_status(target, opts.identity)
}

/// Turns node access control off for now.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_acp_nac_disable_sized(
    target: usize,
    opts: *const DefraNacOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    acp::defra_acp_nac_disable(target, opts.identity)
}

/// Turns node access control back on.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_acp_nac_re_enable_sized(
    target: usize,
    opts: *const DefraNacOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    acp::defra_acp_nac_re_enable(target, opts.identity)
}

/// Removes every node access control relationship (development mode only).
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_acp_nac_purge_sized(
    target: usize,
    opts: *const DefraNacOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    acp::defra_acp_nac_purge(target, opts.identity)
}

/// Gives `actor` (a DID, or `*` for everyone) `relation` on the node.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_acp_nac_relationship_add_sized(
    target: usize,
    relation: *const c_char,
    actor: *const c_char,
    opts: *const DefraNacRelationshipOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    acp::defra_acp_nac_relationship_add(target, opts.identity, relation, actor)
}

/// Takes `relation` on the node from `actor`.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_acp_nac_relationship_delete_sized(
    target: usize,
    relation: *const c_char,
    actor: *const c_char,
    opts: *const DefraNacRelationshipOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    acp::defra_acp_nac_relationship_delete(target, opts.identity, relation, actor)
}

/// Lists the actions running or ended with an error.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_action_list_sized(
    target: usize,
    opts: *const DefraActionListOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    action::defra_action_list(target, opts.identity)
}

/// Starts collecting the blocks the caller's writes create, to sign them
/// together.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_batch_start_sized(
    target: usize,
    opts: *const DefraBatchOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    batch::defra_batch_start(target, opts.identity)
}

/// Signs the blocks collected since `defra_batch_start`.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_batch_sign_sized(
    target: usize,
    opts: *const DefraBatchOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    batch::defra_batch_sign(target, opts.identity)
}

/// Checks that block `cid` was signed by `public_key`, of `key_type`.
///
/// # Safety
///
/// As [`defra_acp_dac_policy_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_block_verify_sized(
    target: usize,
    cid: *const c_char,
    key_type: *const c_char,
    public_key: *const c_char,
    opts: *const DefraBlockVerifyOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    block::defra_block_verify(target, key_type, public_key, cid, opts.identity)
}
