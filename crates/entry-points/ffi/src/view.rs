//! Views: collections defined by a query over other collections.

use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::request::{on_scope, Request};
use crate::target::{go_string, non_empty, CollectionArgs};
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_async_ok, ffi_entry, try_ffi};

/// Adds the views an SDL document defines over the GraphQL `query`, with an
/// optional lens `transform_cid` (null or empty for none), and answers with
/// the created collection versions as JSON. A materialized view is
/// refreshed once created.
///
/// # Safety
///
/// `query`, `sdl` and `transform_cid` must be null or valid null-terminated
/// strings.
pub unsafe fn defra_view_add(
    node_ptr: usize,
    query: *const c_char,
    sdl: *const c_char,
    transform_cid: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::ViewAdd));
        let query = go_string(query);
        let sdl = go_string(sdl);
        let transform = non_empty(transform_cid);
        let node = try_ffi!(request.node_handle());
        let scope = try_ffi!(request.node_scope());

        ffi_async!(caller, {
            let created = on_scope!(node, &scope, |scoped| {
                scoped.add_view(&query, &sdl, transform.as_deref()).await
            })
            .map_err(|e| e.to_string())?;
            serde_json::to_string(&created).map_err(|e| format!("failed to serialize result: {}", e))
        })
    }
}

/// Refreshes the materialized views `opts` selects, every one for empty
/// options.
///
/// # Safety
///
/// Every non-null string in `opts` must be a valid null-terminated string.
pub unsafe fn defra_view_refresh(
    node_ptr: usize,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::ViewRefresh));
        let selector = CollectionArgs::read(&opts).selector();
        let node = try_ffi!(request.node_handle());
        let scope = try_ffi!(request.node_scope());

        ffi_async_ok!(caller, {
            on_scope!(node, &scope, |scoped| scoped.refresh_views(selector).await)
                .map_err(|e| e.to_string())
        })
    }
}
