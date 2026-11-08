//! A C caller's replication filter, given to the node through
//! `NodeInitOptions::replication_filter`.

use std::ffi::{c_char, c_int, CString};
use std::sync::Arc;

use db::replication_filter::{FieldValues, ReplicationFilter};

use crate::types::NodeInitOptions;

/// Whether to keep a replicated document: called with
/// `replication_filter_context`, the collection ID, the collection name (the
/// key a Go filter matches on; the ID stays the same across a rename), the
/// DocID and the values the document's commit writes as a JSON object in DefraDB's
/// JSON encoding (bytes as base64, whole-number floats as integers; an
/// encrypted field, one that has not arrived, or one JSON cannot carry, such
/// as a NaN, is absent). Return non-zero to keep it, 0 to refuse it.
///
/// It is called on the node's database workers and on its network threads,
/// from several threads at once, so it must be thread-safe, and it must not
/// block or call back into DefraDB. One document may be asked about more
/// than once (when it arrives, when it merges, and on a merge retry), so it
/// must decide from its arguments alone. An ID or name holding a NUL, which
/// no valid collection or DocID does, is refused without calling it.
pub type DefraReplicationFilterCallback = unsafe extern "C" fn(
    context: usize,
    collection_id: *const c_char,
    collection_name: *const c_char,
    doc_id: *const c_char,
    fields_json: *const c_char,
) -> c_int;

struct CallbackFilter {
    callback: DefraReplicationFilterCallback,
    context: usize,
}

/// The callback's four string arguments; a field JSON cannot carry is left
/// out, so the callback still decides.
fn arguments(
    collection_id: &str,
    collection_name: &str,
    doc_id: &str,
    fields: &FieldValues,
) -> Result<[CString; 4], String> {
    let fields: serde_json::Map<_, _> = fields
        .iter()
        .filter_map(|(name, value)| match document::cbor_value_to_json(value) {
            Ok(value) => Some((name.clone(), value)),
            Err(error) => {
                tracing::warn!(collection_id, doc_id, field = %name, %error,
                    "replication filter left out a field JSON cannot carry");
                None
            }
        })
        .collect();
    // Serialized JSON escapes a NUL, so only the IDs and name can hold one.
    let fields = serde_json::Value::Object(fields).to_string();
    let c_string = |text: &str| CString::new(text).map_err(|error| error.to_string());
    Ok([
        c_string(collection_id)?,
        c_string(collection_name)?,
        c_string(doc_id)?,
        c_string(&fields)?,
    ])
}

impl ReplicationFilter for CallbackFilter {
    fn allow_replication(
        &self,
        collection_id: &str,
        collection_name: &str,
        doc_id: &str,
        fields: &FieldValues,
    ) -> bool {
        let arguments = arguments(collection_id, collection_name, doc_id, fields);
        let [collection, name, doc, fields] = match arguments {
            Ok(arguments) => arguments,
            Err(error) => {
                tracing::warn!(
                    collection_id,
                    collection_name,
                    doc_id,
                    %error,
                    "replication filter refused a document whose ID or name holds a NUL"
                );
                return false;
            }
        };
        // SAFETY: the caller promised a thread-safe callback when it set it,
        // and the strings live until it returns.
        unsafe {
            (self.callback)(
                self.context,
                collection.as_ptr(),
                name.as_ptr(),
                doc.as_ptr(),
                fields.as_ptr(),
            ) != 0
        }
    }
}

/// `builder` with the replication filter `options` sets, if any.
pub(crate) fn with_replication_filter(
    builder: defra_node::NodeBuilder,
    options: &NodeInitOptions,
) -> defra_node::NodeBuilder {
    match options.replication_filter {
        Some(callback) => builder.with_replication_filter(Arc::new(CallbackFilter {
            callback,
            context: options.replication_filter_context,
        })),
        None => builder,
    }
}
