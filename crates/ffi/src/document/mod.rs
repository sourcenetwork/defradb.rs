//! Documents: added, read, updated, deleted and purged, one by ID or many by
//! filter.

mod add;
mod delete;
mod get;
mod purge;
mod run;
mod save_many;
mod update;

use std::ffi::c_char;

use crate::target::non_empty;
use crate::types::DefraResult;

/// What a call on a document that does not exist, or that the caller may
/// not reach, reports.
pub(crate) const ERR_DOCUMENT_NOT_FOUND: &str = "document not found or not authorized to access";

/// What an update or delete without a document ID or filter reports.
const ERR_NO_DOC_ID_OR_FILTER: &str = "operation requires a DocID or filter";

/// Which documents an update or delete acts on.
enum Selection {
    /// The document with this ID.
    Doc(String),
    /// Every document matching this JSON filter.
    Filter(String),
}

impl Selection {
    /// The selection a call's `doc_id` and `filter` make: the document ID
    /// when there is one, otherwise the filter.
    ///
    /// # Safety
    ///
    /// `doc_id` and `filter` must be null or valid null-terminated strings.
    unsafe fn read(doc_id: *const c_char, filter: *const c_char) -> Result<Self, DefraResult> {
        match (non_empty(doc_id), non_empty(filter)) {
            (Some(doc_id), _) => {
                document::DocID::from_string(&doc_id)
                    .map_err(|e| DefraResult::error(e.to_string()))?;
                Ok(Selection::Doc(doc_id))
            }
            (None, Some(filter)) => Ok(Selection::Filter(filter)),
            (None, None) => Err(DefraResult::error(ERR_NO_DOC_ID_OR_FILTER)),
        }
    }
}

/// A JSON filter, with JSON null meaning every document, as in Go.
fn filter_value(filter: &str) -> Result<serde_json::Value, String> {
    match serde_json::from_str(filter).map_err(|e| format!("invalid filter JSON: {}", e))? {
        serde_json::Value::Null => Ok(serde_json::json!({})),
        filter => Ok(filter),
    }
}

pub use add::defra_document_add;
pub use delete::defra_document_delete;
pub use get::defra_document_get;
pub use purge::defra_document_purge;
pub use save_many::defra_document_save_many;
pub use update::defra_document_update;

#[cfg(test)]
mod signing_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod version_tests;
