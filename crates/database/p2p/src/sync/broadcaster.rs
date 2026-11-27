//! Broadcasting functionality for P2P sync.
//!
//! This module handles broadcasting block updates to the network via GossipSub.
//! Updates are published to both document-specific and collection-specific topics.
//!
//! # Go Compatibility
//!
//! This matches Go's `SendUpdate()` in `p2p.go:532-563` which publishes to both
//! DocID and CollectionID topics.

use bytes::Bytes;
use cid::Cid;

use crate::error::{Error, Result};
use crate::message::PushLogBroadcast;
use crate::topics::DefraTopic;
use crate::transport::{MessageId, P2PTransport};

/// Result of a broadcast operation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BroadcastResult {
    /// Both document and collection topics received the message.
    Success,
    /// Only the document topic received the message.
    PartialDocumentOnly {
        /// Error from the collection topic publish.
        collection_error: String,
        /// Whether that error is gossipsub's "no peer subscribed to this
        /// topic" (`Error::is_no_topic_peer`), which Go publishes to no one
        /// without an error, rather than a publish that failed.
        collection_error_is_no_topic_peer: bool,
    },
    /// Only the collection topic received the message.
    PartialCollectionOnly {
        /// Error from the document topic publish.
        document_error: String,
        /// Whether that error is gossipsub's "no peer subscribed to this
        /// topic" (`Error::is_no_topic_peer`), which Go publishes to no one
        /// without an error, rather than a publish that failed.
        document_error_is_no_topic_peer: bool,
    },
    /// No peer subscribed to either topic, so nothing was sent. Go publishes
    /// such an update to no one, without an error.
    NoSubscribers,
}

/// Broadcaster for sending block updates to the P2P network.
#[derive(Clone)]
pub struct Broadcaster<T: P2PTransport> {
    transport: T,
}

impl<T: P2PTransport> Broadcaster<T> {
    /// Create a new broadcaster.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Subscribe to a collection topic.
    pub async fn subscribe_collection(&self, collection_id: &str) -> Result<bool> {
        let topic = DefraTopic::collection(collection_id);
        self.transport.subscribe(topic).await
    }

    /// Subscribe to a specific document topic.
    pub async fn subscribe_document(&self, doc_id: &str) -> Result<bool> {
        let topic = DefraTopic::document(doc_id);
        self.transport.subscribe(topic).await
    }

    /// Unsubscribe from a collection topic.
    pub async fn unsubscribe_collection(&self, collection_id: &str) -> Result<bool> {
        let topic = DefraTopic::collection(collection_id);
        self.transport.unsubscribe(topic).await
    }

    /// Unsubscribe from a document topic.
    pub async fn unsubscribe_document(&self, doc_id: &str) -> Result<bool> {
        let topic = DefraTopic::document(doc_id);
        self.transport.unsubscribe(topic).await
    }

    /// Broadcast an update to the network.
    ///
    /// This publishes to both the document-specific and collection-specific topics,
    /// matching Go's `SendUpdate()` behavior.
    pub async fn broadcast_update(&self, broadcast: &PushLogBroadcast) -> Result<BroadcastResult> {
        let doc_topic = DefraTopic::document(&broadcast.doc_id);
        let collection_topic = DefraTopic::collection(&broadcast.collection_id);

        let doc_result = self.transport.publish(doc_topic, broadcast.clone()).await;
        let collection_result = self
            .transport
            .publish(collection_topic, broadcast.clone())
            .await;

        classify_publishes(broadcast, &doc_result, &collection_result)
    }

    /// Create a PushLogBroadcast from block data.
    pub fn create_broadcast(
        cid: &Cid,
        block: &[u8],
        doc_id: &str,
        collection_id: &str,
        creator: &str,
    ) -> PushLogBroadcast {
        PushLogBroadcast::new(
            doc_id.to_string(),
            Bytes::from(cid.to_bytes()),
            collection_id.to_string(),
            creator.to_string(),
            Bytes::copy_from_slice(block),
        )
    }

    /// Get the underlying transport reference.
    pub fn transport(&self) -> &T {
        &self.transport
    }
}

/// What a broadcast did, from its two publishes: the document topic's and the
/// collection topic's, in that order. A topic no peer subscribed to is
/// gossipsub's `InsufficientPeers`, which Go publishes to no one without an
/// error; the result names that case in its `*_is_no_topic_peer` field, so a
/// caller decides without reading the error text.
fn classify_publishes(
    broadcast: &PushLogBroadcast,
    doc_result: &Result<MessageId>,
    collection_result: &Result<MessageId>,
) -> Result<BroadcastResult> {
    match (doc_result, collection_result) {
        (Ok(doc_msg_id), Ok(col_msg_id)) => {
            tracing::debug!(
                doc_id = %broadcast.doc_id,
                collection_id = %broadcast.collection_id,
                ?doc_msg_id,
                ?col_msg_id,
                "Broadcast to both topics"
            );
            Ok(BroadcastResult::Success)
        }
        (Ok(_), Err(e)) => {
            let no_topic_peer = e.is_no_topic_peer();
            if no_topic_peer {
                tracing::debug!(
                    doc_id = %broadcast.doc_id,
                    collection_id = %broadcast.collection_id,
                    "No peer subscribed to the collection topic; announced on the document topic"
                );
            } else {
                tracing::warn!(
                    doc_id = %broadcast.doc_id,
                    collection_id = %broadcast.collection_id,
                    error = %e,
                    "Partial broadcast: document topic succeeded, collection topic failed - \
                     some peers may not receive this update"
                );
            }
            Ok(BroadcastResult::PartialDocumentOnly {
                collection_error: e.to_string(),
                collection_error_is_no_topic_peer: no_topic_peer,
            })
        }
        (Err(e), Ok(_)) => {
            let no_topic_peer = e.is_no_topic_peer();
            if no_topic_peer {
                tracing::debug!(
                    doc_id = %broadcast.doc_id,
                    collection_id = %broadcast.collection_id,
                    "No peer subscribed to the document topic; announced on the collection topic"
                );
            } else {
                tracing::warn!(
                    doc_id = %broadcast.doc_id,
                    collection_id = %broadcast.collection_id,
                    error = %e,
                    "Partial broadcast: collection topic succeeded, document topic failed - \
                     some peers may not receive this update"
                );
            }
            Ok(BroadcastResult::PartialCollectionOnly {
                document_error: e.to_string(),
                document_error_is_no_topic_peer: no_topic_peer,
            })
        }
        (Err(doc_err), Err(col_err))
            if doc_err.is_no_topic_peer() && col_err.is_no_topic_peer() =>
        {
            tracing::debug!(
                doc_id = %broadcast.doc_id,
                collection_id = %broadcast.collection_id,
                "No peer subscribed to the write's topics; nothing to announce"
            );
            Ok(BroadcastResult::NoSubscribers)
        }
        (Err(doc_err), Err(col_err)) => {
            if doc_err.is_connection_like() && col_err.is_connection_like() {
                tracing::warn!(
                    doc_id = %broadcast.doc_id,
                    collection_id = %broadcast.collection_id,
                    doc_error = %doc_err,
                    collection_error = %col_err,
                    "Broadcast to both topics failed because the transport was unavailable"
                );
            } else {
                tracing::error!(
                    doc_id = %broadcast.doc_id,
                    collection_id = %broadcast.collection_id,
                    doc_error = %doc_err,
                    collection_error = %col_err,
                    "Failed to broadcast to both topics"
                );
            }
            Err(Error::GossipSubPublish(format!(
                "failed to publish to both topics: doc={}, collection={}",
                doc_err, col_err
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MessageId;

    fn broadcast() -> PushLogBroadcast {
        PushLogBroadcast::new(
            "doc-1".to_string(),
            Bytes::from_static(&[0x01]),
            "collection-1".to_string(),
            "did:key:creator".to_string(),
            Bytes::from_static(b"block"),
        )
    }

    fn no_topic_peer() -> Result<MessageId> {
        Err(Error::GossipSubPublish("InsufficientPeers".to_string()))
    }

    fn published() -> Result<MessageId> {
        Ok(MessageId::new("message".to_string()))
    }

    /// The inverse of the collection-only case: no peer on the collection
    /// topic, so the write is announced on the document topic alone, and the
    /// result says the collection error was a topic with no peer rather than
    /// a publish that failed.
    #[test]
    fn a_collection_topic_with_no_peer_is_reported_as_a_no_topic_peer() {
        let result = classify_publishes(&broadcast(), &published(), &no_topic_peer()).unwrap();

        let BroadcastResult::PartialDocumentOnly {
            collection_error,
            collection_error_is_no_topic_peer,
        } = result
        else {
            panic!("expected a document-only broadcast, got {result:?}");
        };
        assert!(collection_error.contains("InsufficientPeers"));
        assert!(collection_error_is_no_topic_peer);
    }

    /// A collection topic whose publish really failed is not a no-topic-peer
    /// case: a caller must report it.
    #[test]
    fn a_failed_collection_publish_is_not_a_no_topic_peer() {
        let result = classify_publishes(
            &broadcast(),
            &published(),
            &Err(Error::GossipSubPublish("MessageTooLarge".to_string())),
        )
        .unwrap();

        let BroadcastResult::PartialDocumentOnly {
            collection_error_is_no_topic_peer,
            ..
        } = result
        else {
            panic!("expected a document-only broadcast, got {result:?}");
        };
        assert!(!collection_error_is_no_topic_peer);
    }

    #[test]
    fn a_document_topic_with_no_peer_is_reported_as_a_no_topic_peer() {
        let result = classify_publishes(&broadcast(), &no_topic_peer(), &published()).unwrap();

        let BroadcastResult::PartialCollectionOnly {
            document_error_is_no_topic_peer,
            ..
        } = result
        else {
            panic!("expected a collection-only broadcast, got {result:?}");
        };
        assert!(document_error_is_no_topic_peer);
    }

    #[test]
    fn no_peer_on_either_topic_reports_no_subscribers() {
        let result = classify_publishes(&broadcast(), &no_topic_peer(), &no_topic_peer()).unwrap();

        assert_eq!(result, BroadcastResult::NoSubscribers);
    }
}
