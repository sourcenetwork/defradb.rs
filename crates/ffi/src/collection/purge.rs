use std::ffi::c_char;

use serde_json::json;

use crate::helpers::require_c_str;
use crate::types::{c_str_to_string, FfiResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Delete documents by their docIDs, through the core's filtered delete on
/// the node's request path.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `identity_did` - Optional identity DID (nullable)
/// * `collection_name` - The collection to delete from
/// * `doc_ids_json` - JSON array of docID strings: `["bae-...", "bae-..."]`
///
/// # Returns
///
/// - Status 0: Success (value is `{"deleted": N}`)
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `collection_name` and `doc_ids_json` must be valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn delete_documents(
    node_ptr: usize,
    identity_did: *const c_char,
    collection_name: *const c_char,
    doc_ids_json: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let identity = c_str_to_string(identity_did);
        let col_name = try_ffi!(require_c_str(collection_name, "collection_name"));
        let ids_json = try_ffi!(require_c_str(doc_ids_json, "doc_ids_json"));
        let doc_ids: Vec<String> = try_ffi!(serde_json::from_str(&ids_json)
            .map_err(|e| FfiResult::error(format!("invalid doc_ids_json: {}", e))));

        ffi_async!(caller, {
            if doc_ids.is_empty() {
                return Ok(json!({"deleted": 0}).to_string());
            }
            let filter = query::documents::Filter::from(json!({"_docID": {"_in": doc_ids}}));
            let run = |mutation| crate::query::run_mutation(node_ptr, identity, None, mutation);
            let deleted = query::documents::delete_with_filter(run, &col_name, &filter)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({"deleted": deleted.count}).to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{new_node, node_close};
    use crate::query::exec_request;
    use crate::schema::add_schema;
    use crate::types::NodeInitOptions;
    use std::ffi::CString;

    #[test]
    fn test_delete_documents_empty_array() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        // Add schema
        let sdl = CString::new("type PurgeTest { name: String }").unwrap();
        let result = unsafe { add_schema(node, std::ptr::null(), sdl.as_ptr()) };
        assert_eq!(result.status, 0);
        unsafe { crate::types::defra_free_string(result.value) };

        // Delete with empty array
        let col = CString::new("PurgeTest").unwrap();
        let ids = CString::new("[]").unwrap();
        let result =
            unsafe { delete_documents(node, std::ptr::null(), col.as_ptr(), ids.as_ptr()) };
        assert_eq!(result.status, 0);
        let value = unsafe { std::ffi::CStr::from_ptr(result.value).to_string_lossy() };
        assert_eq!(value, "{\"deleted\":0}");
        unsafe { crate::types::defra_free_string(result.value) };

        node_close(node);
    }

    #[test]
    fn test_delete_documents_with_data() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        // Add schema
        let sdl = CString::new("type PurgeData { name: String }").unwrap();
        let result = unsafe { add_schema(node, std::ptr::null(), sdl.as_ptr()) };
        assert_eq!(result.status, 0);
        unsafe { crate::types::defra_free_string(result.value) };

        // Create a document
        let mutation =
            CString::new(r#"mutation { add_PurgeData(input: {name: "test"}) { _docID } }"#)
                .unwrap();
        let result = unsafe {
            exec_request(
                node,
                std::ptr::null(),
                std::ptr::null(),
                mutation.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_eq!(result.status, 0);
        let resp = unsafe { std::ffi::CStr::from_ptr(result.value).to_string_lossy() };

        // Extract docID from response
        let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
        let doc_id = parsed["data"]["add_PurgeData"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_string();
        unsafe { crate::types::defra_free_string(result.value) };

        // Delete the document
        let col = CString::new("PurgeData").unwrap();
        let ids = CString::new(format!("[\"{}\"]", doc_id)).unwrap();
        let result =
            unsafe { delete_documents(node, std::ptr::null(), col.as_ptr(), ids.as_ptr()) };
        assert_eq!(result.status, 0);
        let value = unsafe { std::ffi::CStr::from_ptr(result.value).to_string_lossy() };
        assert_eq!(value, "{\"deleted\":1}");
        unsafe { crate::types::defra_free_string(result.value) };

        node_close(node);
    }
}
