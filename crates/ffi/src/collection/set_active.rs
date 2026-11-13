use acp::nac::NodePermission;

use crate::request::Request;
use crate::target::CollectionArgs;
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async_ok, ffi_entry, try_ffi};

/// Makes the collection version `opts.version` names active, deactivating
/// the other versions of its collection, on a node or in a transaction.
///
/// # Safety
///
/// Every non-null string in `opts` must be a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_collection_set_active(
    node_ptr: usize,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::CollectionPatch));
        let version_id = CollectionArgs::read(&opts).version.unwrap_or_default();
        let context = request.context();
        let runner = try_ffi!(request.runner());
        ffi_async_ok!(caller, {
            runner
                .set_active_collection_version_as(&context, &version_id)
                .await
                .map_err(|e| e.to_string())
        })
    }
}
