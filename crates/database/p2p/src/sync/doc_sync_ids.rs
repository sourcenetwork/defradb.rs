//! The documents a doc sync request covers, the same on every path that serves it.

use super::head_provider::DocumentHeadProvider;
use crate::error::{Error, Result};

/// The requested documents, then the documents that sync with each of them,
/// or a refusal once they are more than `max`.
///
/// Each one costs a head lookup and a reply item, so a parent's chunks count
/// too. The lookups stop at the first parent that goes past `max`.
pub(crate) async fn with_followers<P>(
    provider: &P,
    doc_ids: &[String],
    max: usize,
) -> Result<Vec<String>>
where
    P: DocumentHeadProvider + ?Sized,
{
    if doc_ids.len() > max {
        return Err(Error::InvalidConfig(format!(
            "DocSyncRequest contains {} doc IDs, exceeding the limit of {max}",
            doc_ids.len()
        )));
    }
    let mut all = doc_ids.to_vec();
    for doc_id in doc_ids {
        match provider.following_doc_ids(doc_id).await {
            Ok(followers) => all.extend(followers),
            Err(e) => {
                tracing::warn!(doc_id = %doc_id, error = %e, "Failed to find documents following a DocSync document");
            }
        }
        if all.len() > max {
            return Err(Error::InvalidConfig(format!(
                "DocSyncRequest with its documents' chunks exceeds the limit of {max} doc IDs"
            )));
        }
    }
    Ok(all)
}

#[cfg(test)]
#[path = "doc_sync_ids_tests.rs"]
mod tests;
