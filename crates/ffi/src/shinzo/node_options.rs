//! The Shinzo node options of `NodeInitOptions`: Go Shinzo's wire, the
//! retention rule and the replication filter.

use std::ffi::{c_char, c_int, CString};
use std::sync::Arc;

use db::replication_filter::{FieldValues, ReplicationFilter};
use db::retention::{RetentionFloor, StaticRetentionRule};
use rapidhash::{HashMapExt, RapidHashMap};

use crate::types::{c_str_to_string, NodeInitOptions};

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

/// A C caller's replication filter.
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

/// `builder` with the Shinzo options `options` sets.
///
/// # Safety
///
/// `options.retention_json` must be null or a valid null-terminated UTF-8
/// string, and `options.replication_filter` thread-safe as its type says.
pub unsafe fn with_node_options(
    mut builder: defra_node::NodeBuilder,
    options: &NodeInitOptions,
) -> Result<defra_node::NodeBuilder, String> {
    if options.shinzo_wire != 0 {
        builder = builder.with_shinzo_wire();
    }
    if let Some(json) = c_str_to_string(options.retention_json).filter(|json| !json.is_empty()) {
        builder = builder.with_retention(retention_rule(&json)?);
    }
    if let Some(callback) = options.replication_filter {
        builder = builder.with_replication_filter(Arc::new(CallbackFilter {
            callback,
            context: options.replication_filter_context,
        }));
    }
    Ok(builder)
}

/// The rule `{"<collectionID>": {"field": …, "floor": …}}` names.
fn retention_rule(json: &str) -> Result<StaticRetentionRule, String> {
    #[derive(serde::Deserialize)]
    struct Floor {
        field: String,
        floor: i64,
    }
    let parsed: std::collections::BTreeMap<String, Floor> =
        serde_json::from_str(json).map_err(|error| format!("invalid retention_json: {error}"))?;
    let mut floors = RapidHashMap::with_capacity(parsed.len());
    for (collection_id, Floor { field, floor }) in parsed {
        floors.insert(collection_id, RetentionFloor { field, floor });
    }
    Ok(StaticRetentionRule::new(floors))
}
