use std::ffi::c_char;

use acp::nac::NodePermission;

use super::run::DocumentCall;
use super::{filter_value, Selection};
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Deletes the document `doc_id` of the collection `opts` selects, answering
/// with no value; or, without a document ID, every document matching the
/// JSON `filter`, answering with `{"Count": ..., "DocIDs": [...]}`.
///
/// # Safety
///
/// `doc_id` and `filter` must be null or valid null-terminated strings;
/// every non-null string in `opts` must be one too.
#[no_mangle]
pub unsafe extern "C" fn defra_document_delete(
    node_ptr: usize,
    doc_id: *const c_char,
    filter: *const c_char,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let selection = try_ffi!(Selection::read(doc_id, filter));
        let call = try_ffi!(DocumentCall::begin(node_ptr, &opts, identity_ptr, NodePermission::DocumentDelete));
        let target = call.target().clone();
        ffi_async!(target &target, {
            let name = call.collection().await?.name;
            let run = |mutation| call.respond(mutation);
            match selection {
                Selection::Doc(doc_id) => {
                    query::documents::delete(run, &name, &doc_id)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(String::new())
                }
                Selection::Filter(filter) => {
                    let filter = query::documents::Filter::from(filter_value(&filter)?);
                    let deleted = query::documents::delete_with_filter(run, &name, &filter)
                        .await
                        .map_err(|e| e.to_string())?;
                    serde_json::to_string(&deleted).map_err(|e| e.to_string())
                }
            }
        })
    }
}
