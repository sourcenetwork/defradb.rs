//! The documents a doc sync request covers, the same on every path that serves it.

use rapidhash::RapidHashSet;

use super::head_provider::DocumentHeadProvider;
use crate::error::Error;

/// A doc sync request that covers more documents than the node serves at once.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum TooMany {
    #[error("DocSyncRequest contains {count} doc IDs, exceeding the limit of {max}")]
    Requested { count: usize, max: usize },
    #[error("DocSyncRequest with its documents' chunks exceeds the limit of {max} doc IDs")]
    WithChunks { count: usize, max: usize },
}

impl TooMany {
    /// How many documents the request had reached when it went over.
    pub(crate) fn count(&self) -> usize {
        match self {
            Self::Requested { count, .. } | Self::WithChunks { count, .. } => *count,
        }
    }
}

impl From<TooMany> for Error {
    fn from(refusal: TooMany) -> Self {
        Error::InvalidConfig(refusal.to_string())
    }
}

/// The requested documents, then the chunks a peer can't make, each once; refused past `max`.
/// Each costs a head lookup and a reply item, so the chunks count too.
pub(crate) async fn with_followers<P>(
    provider: &P,
    doc_ids: &[String],
    max: usize,
) -> std::result::Result<Vec<String>, TooMany>
where
    P: DocumentHeadProvider + ?Sized,
{
    let mut seen: RapidHashSet<String> = RapidHashSet::default();
    let mut all: Vec<String> = Vec::with_capacity(doc_ids.len());
    for doc_id in doc_ids {
        if seen.insert(doc_id.clone()) {
            all.push(doc_id.clone());
        }
    }
    if all.len() > max {
        return Err(TooMany::Requested {
            count: all.len(),
            max,
        });
    }
    let requested = all.clone();
    for doc_id in &requested {
        match provider.following_doc_ids(doc_id).await {
            Ok(followers) => all.extend(
                followers
                    .into_iter()
                    .filter(|follower| seen.insert(follower.clone())),
            ),
            Err(e) => {
                tracing::warn!(doc_id = %doc_id, error = %e, "Failed to find documents following a DocSync document");
            }
        }
        if all.len() > max {
            return Err(TooMany::WithChunks {
                count: all.len(),
                max,
            });
        }
    }
    Ok(all)
}

#[cfg(test)]
#[path = "doc_sync_ids_tests.rs"]
mod tests;
