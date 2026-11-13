use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::request::Request;
use crate::target::{go_string, non_empty};
use crate::types::DefraResult;
use crate::{ffi_async_ok, ffi_entry, try_ffi};

/// Applies a JSON Patch (RFC 6902) to the collection definitions, with an
/// optional migration lens (`{"Lenses": [...]}`, null or empty for none).
/// The patch's first operation names the collection: `/Users/Fields/-`
/// patches `Users`.
///
/// # Safety
///
/// `patch` and `lens_config` must be null or valid null-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn defra_collection_patch(
    node_ptr: usize,
    patch: *const c_char,
    lens_config: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::CollectionPatch));
        try_ffi!(request.require_node("collection patch"));
        let runner = try_ffi!(request.runner());
        let context = request.context();
        let patch = go_string(patch);
        let lens_config = non_empty(lens_config);
        ffi_async_ok!(caller, {
            let migration = lens_config
                .map(|config| serde_json::from_str::<lens::LensConfig>(&config))
                .transpose()
                .map_err(|e| format!("invalid lens configuration: {}", e))?;
            runner
                .patch_collection_as(&context, &patch, migration)
                .await
                .map_err(|e| e.to_string())
        })
    }
}
