//! The Shinzo node options of `NodeInitOptions`: Go Shinzo's wire, the
//! retention rule and the replication filter.

use std::ffi::{c_char, c_int, CString};
use std::sync::Arc;

use db::replication_filter::{FieldValues, ReplicationFilter};
use db::retention::{RetentionFloor, RetentionRule, StaticRetentionRule};
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
        #[cfg(feature = "libp2p")]
        {
            builder = builder.with_shinzo_wire();
        }
        #[cfg(not(feature = "libp2p"))]
        return Err(
            "shinzo_wire needs the libp2p transport, which this build leaves out".to_owned(),
        );
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
    if let Some(callback) = options.retention_rule {
        builder = builder.with_retention_rule(Arc::new(CallbackRetention {
            callback,
            context: options.retention_rule_context,
        }));
    }
    Ok(builder)
}

/// A C caller's retention rule: `NodeInitOptions::retention_rule`.
type RetentionCallback = unsafe extern "C" fn(
    context: usize,
    collection_id: *const c_char,
    field_out: *mut c_char,
    field_capacity: usize,
    floor_out: *mut i64,
) -> c_int;

struct CallbackRetention {
    callback: RetentionCallback,
    context: usize,
}

/// The longest height field name the callback may write.
const FIELD_CAPACITY: usize = 256;

impl RetentionRule for CallbackRetention {
    fn retention_floor(&self, collection_id: &str) -> Option<RetentionFloor> {
        let collection_id = CString::new(collection_id).ok()?;
        let mut field = [0 as c_char; FIELD_CAPACITY];
        let mut floor = 0i64;
        // SAFETY: the caller promised a thread-safe callback that writes at
        // most `FIELD_CAPACITY` bytes, null-terminated, and one i64.
        let covered = unsafe {
            (self.callback)(
                self.context,
                collection_id.as_ptr(),
                field.as_mut_ptr(),
                FIELD_CAPACITY,
                &mut floor,
            )
        };
        if covered == 0 {
            return None;
        }
        field[FIELD_CAPACITY - 1] = 0;
        // SAFETY: the buffer is null-terminated just above.
        let field = unsafe { std::ffi::CStr::from_ptr(field.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        Some(RetentionFloor { field, floor })
    }
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
