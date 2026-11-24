//! The documents a doc sync request covers, the same on every path that serves it.

use super::head_provider::DocumentHeadProvider;
use crate::error::{Error, Result};

/// Refuses a request that names more than `max` documents.
///
/// Only the requested IDs count, so a parent with many chunks still syncs.
pub(crate) fn check_doc_id_limit(requested: usize, max: usize) -> Result<()> {
    if requested > max {
        return Err(Error::InvalidConfig(format!(
            "DocSyncRequest contains {requested} doc IDs, exceeding the limit of {max}"
        )));
    }
    Ok(())
}

/// The requested documents, then the documents that sync with each of them.
pub(crate) async fn with_followers<P>(provider: &P, doc_ids: &[String]) -> Vec<String>
where
    P: DocumentHeadProvider + ?Sized,
{
    let mut all = doc_ids.to_vec();
    for doc_id in doc_ids {
        match provider.following_doc_ids(doc_id).await {
            Ok(followers) => all.extend(followers),
            Err(e) => {
                tracing::warn!(doc_id = %doc_id, error = %e, "Failed to find documents following a DocSync document");
            }
        }
    }
    all
}

#[cfg(test)]
#[path = "doc_sync_ids_tests.rs"]
mod tests;
