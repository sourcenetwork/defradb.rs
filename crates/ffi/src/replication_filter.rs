//! A C caller's replication filter, given to the node through
//! `NodeInitOptions::replication_filter`.

use std::ffi::{c_char, c_int, CString};
use std::sync::Arc;

use db::replication_filter::{FieldValues, ReplicationFilter};

use crate::types::NodeInitOptions;

/// Whether to keep a replicated document: called with
/// `replication_filter_context`, the collection ID, the DocID and the
/// values the document's commit writes as a JSON object (an encrypted field,
/// or one that has not arrived, is absent). Return non-zero to keep it, 0 to
/// refuse it. It is called on the node's database workers, from several
/// threads at once, so it must be thread-safe, and it must not block: it
/// decides from what it is given.
pub type DefraReplicationFilterCallback = unsafe extern "C" fn(
    context: usize,
    collection_id: *const c_char,
    doc_id: *const c_char,
    fields_json: *const c_char,
) -> c_int;

struct CallbackFilter {
    callback: DefraReplicationFilterCallback,
    context: usize,
}

impl ReplicationFilter for CallbackFilter {
    fn allow_replication(&self, collection_id: &str, doc_id: &str, fields: &FieldValues) -> bool {
        let fields = serde_json::to_string(fields).unwrap_or_else(|_| "{}".to_owned());
        let (Ok(collection_id), Ok(doc_id), Ok(fields)) = (
            CString::new(collection_id),
            CString::new(doc_id),
            CString::new(fields),
        ) else {
            return true;
        };
        // SAFETY: the caller promised a thread-safe callback when it set it,
        // and the strings live until it returns.
        unsafe {
            (self.callback)(
                self.context,
                collection_id.as_ptr(),
                doc_id.as_ptr(),
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
