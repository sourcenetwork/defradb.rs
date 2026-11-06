//! Views: collections defined by a query over other collections.

use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::request::Request;
use crate::state::NODES;
use crate::target::{go_string, non_empty, CollectionArgs};
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_async_ok, ffi_entry, try_ffi};
use query::select_to_go_json;

/// Adds the views an SDL document defines over the GraphQL `query`, with an
/// optional lens `transform_cid` (null or empty for none), and answers with
/// the created collection versions as JSON. A materialized view is
/// refreshed once created.
///
/// # Safety
///
/// `query`, `sdl` and `transform_cid` must be null or valid null-terminated
/// strings.
#[no_mangle]
pub unsafe extern "C" fn defra_view_add(
    node_ptr: usize,
    query: *const c_char,
    sdl: *const c_char,
    transform_cid: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::ViewAdd));
        let node = try_ffi!(request.require_node("view add"));
        let query_str = go_string(query);
        let sdl_str = go_string(sdl);
        let transform_opt = non_empty(transform_cid);
        let creator = try_ffi!(request.did());
        let context = request.context();
        let (database, query_limits, document_acp) = try_ffi!(NODES
            .get(node, |state| {
                (
                    state.database.clone(),
                    state.query_limits,
                    state.document_acp.clone(),
                )
            })
            .ok_or_else(|| DefraResult::error(crate::ERR_INVALID_NODE_HANDLE)));

        ffi_async!(node, {
            // Get existing collection names so the SDL parser can resolve external type references
            let known_types: rapidhash::RapidHashSet<String> = database
                .list_collections()
                .unwrap_or_default()
                .into_iter()
                .collect();

            // Parse the SDL into collection versions, passing known types for resolution
            let collections = query::parse_sdl_with_known_types(&sdl_str, known_types)
                .map_err(|e| format!("failed to parse view SDL: {}", e))?;

            // Parse the GQL query into a Select, matching Go's `addView` behavior:
            // Go wraps query as `query { <input> }` then parses to request.Select
            let wrapped_query = format!("query {{ {} }}", query_str);
            let selects = query::parse_query_with_limits(&wrapped_query, None, query_limits)
                .map_err(|e| format!("failed to parse view query: {}", e))?;
            if selects.is_empty() {
                return Err("invalid view query: no selections found".to_string());
            }
            let select_json = select_to_go_json(&selects[0]);

            // Validate transform CID exists in the lens store if provided
            if let Some(ref t) = transform_opt {
                let lens_store = database.lens_store();
                // Check each transform ID (may be comma-separated for chained transforms)
                for cid in t.split(',') {
                    let cid = cid.trim();
                    if !cid.is_empty() {
                        let tid = lens::TransformId::new(cid);
                        if !lens_store.has_transform(&tid) {
                            return Err("lens CID not found".to_string());
                        }
                    }
                }
            }

            // Build the query source with Go-compatible Select JSON
            let mut query_source = schema::QuerySource::new(select_json);
            if let Some(ref t) = transform_opt {
                query_source = query_source.with_transform(t);
            }

            // Attach query source to all collections
            let view_collections: Vec<_> = collections
                .into_iter()
                .map(|mut col_version| {
                    if col_version.downsample_interval.is_some() {
                        col_version.downsample_source = Some(query_source.clone());
                    } else {
                        col_version.query = Some(query_source.clone());
                    }
                    col_version
                })
                .collect();

            for collection in &view_collections {
                if collection.downsample_interval.is_some() {
                    database
                        .validate_downsample_collection(collection)
                        .map_err(|e| format!("invalid downsample definition: {}", e))?;
                }
            }

            // Create all view collections atomically (all-or-nothing)
            let created_versions = database
                .create_collections_atomic_with_acp_registration_as(&context, view_collections, document_acp.clone(), creator)
                .await
                .map_err(|e| format!("failed to create view collection: {}", e))?;

            // Auto-refresh materialized views after creation
            // Exclude embedded-only types (interfaces) - they can't be queried
            let materialized_names: Vec<String> = created_versions
                .iter()
                .filter(|col| col.query.is_some() && col.is_materialized && !col.is_embedded_only)
                .map(|col| col.name.clone())
                .collect();
            let downsample_names: Vec<String> = created_versions
                .iter()
                .filter(|col| col.downsample_interval.is_some())
                .map(|col| col.name.clone())
                .collect();

            if !materialized_names.is_empty() {
                database
                    .refresh_views(db::CollectionSelector::with_names(
                        materialized_names,
                    ))
                    .await
                    .map_err(|e| format!("failed to refresh materialized views: {}", e))?;
            }

            if !downsample_names.is_empty() {
                database
                    .bootstrap_downsamples_as(&context, Some(&downsample_names))
                    .await
                    .map_err(|e| format!("failed to bootstrap downsample collections: {}", e))?;
            }

            let json = serde_json::to_string(&created_versions)
                .map_err(|e| format!("failed to serialize result: {}", e))?;

            Ok(json)
        })
    }
}

/// Refresh view caches.
///
/// Refreshes the caches of all materialized views matching the given options.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `options` - JSON string with optional "Names" field (null for all views)
///
/// # Returns
///
/// - Status 0: Success (value is "{}")
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `options` must be null or a valid null-terminated UTF-8 string.
/// Refreshes the materialized views `opts` selects, every one for empty
/// options.
///
/// # Safety
///
/// Every non-null string in `opts` must be a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_view_refresh(
    node_ptr: usize,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::ViewRefresh));
        let node = try_ffi!(request.require_node("view refresh"));
        let database = try_ffi!(request.database());
        let selector = CollectionArgs::read(&opts).selector();
        ffi_async_ok!(node, {
            database
                .refresh_views(selector)
                .await
                .map_err(|e| format!("failed to refresh views: {}", e))
        })
    }
}
