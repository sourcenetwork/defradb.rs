//! The P2P operations view of one peer: the sync layer's details plus the
//! peer's identity and the durable push retries this node owes it.

use chrono::{DateTime, Utc};

use crate::{P2PError, P2PErrorExt as _, P2PResult};

/// Durable push retries this node still owes a peer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PeerPushRetries {
    pub documents: usize,
    pub collections: usize,
    /// Highest retry count across the peer's scopes.
    pub most_attempts: u32,
    /// When the next retry is due, backpressure deadlines included.
    pub next_retry_at: Option<DateTime<Utc>>,
}

impl PeerPushRetries {
    pub(crate) fn from_markers(markers: &[storage::stores::PushRetryMarker]) -> Option<Self> {
        if markers.is_empty() {
            return None;
        }
        let collections = markers
            .iter()
            .filter(|marker| marker.scope == storage::stores::RetryScope::CollectionCommit)
            .count();
        let next_retry_unix = markers
            .iter()
            .map(|marker| {
                marker
                    .retry_info
                    .next_retry_unix
                    .max(marker.retry_info.not_before_unix)
            })
            .min();
        Some(Self {
            documents: markers.len() - collections,
            collections,
            most_attempts: markers
                .iter()
                .map(|marker| marker.retry_info.num_retries)
                .max()
                .unwrap_or(0),
            next_retry_at: next_retry_unix
                .and_then(|secs| DateTime::from_timestamp(secs as i64, 0)),
        })
    }
}

/// Add the identity and push retries to the sync layer's details. A source
/// that fails is named in `source_errors` instead of failing the call.
pub(crate) fn peer_details_json(
    details: p2p::sync::PeerDetails,
    identity: Option<P2PResult<Option<identity::Did>>>,
    push_retries: P2PResult<Option<PeerPushRetries>>,
) -> P2PResult<serde_json::Value> {
    let mut value =
        serde_json::to_value(details).map_err(|error| P2PError::internal(error.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| P2PError::internal("peer details must serialize to an object"))?;
    let mut source_errors = Vec::new();
    let did = match identity {
        Some(Ok(did)) => did.map(|did| did.to_string()),
        Some(Err(error)) => {
            source_errors.push(format!("identity: {error}"));
            None
        }
        None => None,
    };
    object.insert("did".to_string(), serde_json::json!(did));
    let push_retries = push_retries.unwrap_or_else(|error| {
        source_errors.push(format!("push_retries: {error}"));
        None
    });
    object.insert("push_retries".to_string(), serde_json::json!(push_retries));
    if let Some(serde_json::Value::Array(errors)) = object.get_mut("source_errors") {
        errors.extend(source_errors.into_iter().map(serde_json::Value::String));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::stores::{PushRetryMarker, RetryInfo, RetryScope};

    fn marker(scope: RetryScope, num_retries: u32, next_retry_unix: u64) -> PushRetryMarker {
        PushRetryMarker {
            doc_id: "doc".to_string(),
            collection_id: "col".to_string(),
            scope,
            retry_info: RetryInfo {
                num_retries,
                next_retry_unix,
                ..RetryInfo::new_initial()
            },
            generation: 1,
        }
    }

    fn details() -> p2p::sync::PeerDetails {
        p2p::sync::PeerDetails {
            peer_id: "peer".to_string(),
            history: p2p::PeerHistory::default(),
            connection: None,
            replicated_collections: Vec::new(),
            push_queue: None,
            pending_fetch_count: 0,
            pending_fetches: Vec::new(),
            quarantined: Vec::new(),
            car_fetches: None,
            source_errors: Vec::new(),
        }
    }

    #[test]
    fn push_retries_summarize_scopes_attempts_and_the_next_due_time() {
        assert_eq!(PeerPushRetries::from_markers(&[]), None);
        let retries = PeerPushRetries::from_markers(&[
            marker(RetryScope::Document, 2, 2_000_000_000),
            marker(RetryScope::Document, 5, 1_900_000_000),
            marker(RetryScope::CollectionCommit, 1, 2_100_000_000),
        ])
        .unwrap();
        assert_eq!(retries.documents, 2);
        assert_eq!(retries.collections, 1);
        assert_eq!(retries.most_attempts, 5);
        assert_eq!(
            retries.next_retry_at,
            DateTime::from_timestamp(1_900_000_000, 0)
        );
    }

    #[test]
    fn details_json_adds_did_and_retries_and_names_failed_sources() {
        let value = peer_details_json(
            details(),
            Some(Err(P2PError::transport("identity challenge timed out"))),
            Ok(PeerPushRetries::from_markers(&[marker(
                RetryScope::Document,
                3,
                1_900_000_000,
            )])),
        )
        .unwrap();
        assert!(value["did"].is_null());
        assert_eq!(value["push_retries"]["most_attempts"], 3);
        assert_eq!(value["blocks_received"]["succeeded"], 0);
        let errors = value["source_errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].as_str().unwrap().starts_with("identity: "));

        let unknown =
            peer_details_json(details(), None, Err(P2PError::internal("store closed"))).unwrap();
        assert!(unknown["push_retries"].is_null());
        assert_eq!(unknown["source_errors"].as_array().unwrap().len(), 1);
    }
}
