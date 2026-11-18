use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::request::Request;
use crate::target::{go_string, CollectionArgs};
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Deletes the index `index_name` from the collection `opts.name` names.
///
/// # Safety
///
/// `index_name` must be null or a valid null-terminated string; every
/// non-null string in `opts` must be one too.
#[no_mangle]
pub unsafe extern "C" fn defra_index_delete(
    node_ptr: usize,
    index_name: *const c_char,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::IndexDelete));
        let collection_name = CollectionArgs::read(&opts).name.unwrap_or_default();
        let index_name = go_string(index_name);
        let node = try_ffi!(request.node_handle());
        let context = request.context();

        ffi_async!(caller, {
            node.delete_index_as(&context, &collection_name, &index_name)
                .await
                .map_err(|e| e.to_string())?;
            Ok("{}".to_string())
        })
    }
}

/// Answers with the indexes of the collection `opts.name` names as a JSON
/// array, or, when it names none, with every collection's indexes as a JSON
/// object keyed by collection name, leaving out collections without any.
///
/// # Safety
///
/// Every non-null string in `opts` must be a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_index_list(
    node_ptr: usize,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::IndexList));
        let collection_name = CollectionArgs::read(&opts).name;
        let node = try_ffi!(request.node_handle());
        let context = request.context();

        ffi_async!(caller, {
            let json = match collection_name {
                Some(name) => {
                    let indexes = node
                        .list_indexes_as(&context, &name)
                        .await
                        .map_err(|e| e.to_string())?;
                    serde_json::to_string(&indexes)
                }
                None => {
                    let collections = node
                        .get_collections_as(&context, &db::CollectionSelector::all())
                        .await
                        .map_err(|e| e.to_string())?;
                    let mut all = rapidhash::RapidHashMap::default();
                    for collection in collections {
                        let indexes = node
                            .list_indexes_as(&context, &collection.name)
                            .await
                            .map_err(|e| e.to_string())?;
                        if !indexes.is_empty() {
                            all.insert(collection.name, indexes);
                        }
                    }
                    serde_json::to_string(&all)
                }
            };
            json.map_err(|e| format!("failed to serialize result: {}", e))
        })
    }
}
