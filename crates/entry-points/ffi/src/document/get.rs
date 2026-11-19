use std::ffi::{c_char, c_int};

use acp::nac::NodePermission;

use super::run::DocumentCall;
use super::ERR_DOCUMENT_NOT_FOUND;
use crate::target::go_string;
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Answers with the document `doc_id` of the collection `opts` selects as a
/// JSON object of its fields and `_docID`; with `show_deleted` set, a
/// deleted document too.
///
/// # Safety
///
/// `doc_id` must be null or a valid null-terminated string; every non-null
/// string in `opts` must be one too.
#[no_mangle]
pub unsafe extern "C" fn defra_document_get(
    node_ptr: usize,
    doc_id: *const c_char,
    show_deleted: c_int,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let call = try_ffi!(DocumentCall::begin(node_ptr, &opts, identity_ptr, NodePermission::DocumentRead));
        let doc_id = go_string(doc_id);
        let show_deleted = show_deleted != 0;
        let target = call.target().clone();
        ffi_async!(target &target, {
            document::DocID::from_string(&doc_id).map_err(|e| e.to_string())?;
            let collection = call.collection().await?;
            let fields: Vec<&str> = std::iter::once("_docID")
                .chain(
                    collection
                        .fields
                        .iter()
                        // Go's document holds no secondary relation ID, and
                        // refuses one when the caller sets it back.
                        .filter(|field| {
                            field.name != "_docID"
                                && field.kind.is_scalar()
                                && !field.is_secondary_relation()
                        })
                        .map(|field| field.name.as_str()),
                )
                .collect();
            let query = format!(
                "{{ {name}(docID: {doc_id}, showDeleted: {show_deleted}) {{ {fields} }} }}",
                name = collection.name,
                doc_id = serde_json::Value::from(doc_id.as_str()),
                fields = fields.join(" "),
            );
            let data = call.run(query).await?;
            let doc = data
                .get(&collection.name)
                .and_then(|docs| docs.as_array())
                .and_then(|docs| docs.first())
                .ok_or(ERR_DOCUMENT_NOT_FOUND)?;
            Ok(doc.to_string())
        })
    }
}
