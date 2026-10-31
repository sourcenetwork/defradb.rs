//! Go Shinzo's gossip batches in the host: inline CARs kept until the
//! coordinator asks for their heads, and this node's collection-topic
//! updates published in Go's `documents` shape. On only with
//! `P2PHostConfig::shinzo_wire`.

use std::time::Instant;

use ::bitswap::Store;
use bytes::Bytes;
use cid::Cid;
use libp2p::gossipsub::{MessageId, PublishError};
use libp2p::PeerId;
use tracing::{debug, warn};

use super::P2PHost;
use crate::error::Result;
use crate::host::event::HostEvent;
use crate::message::{DocumentInfo, PushLogBroadcast};
use crate::sync::shinzo_batch::{encode_batch, Batch};
use crate::topics::DefraTopic;

impl<S> P2PHost<S>
where
    S: Store,
{
    /// Keep a CAR that came inline with an update for `cid`'s head, bound
    /// to `offered_by`, the authenticated peer it came from. A CAR with no
    /// authenticated sender is not kept.
    pub(crate) fn offer_inline_car(&mut self, cid: &[u8], offered_by: Option<PeerId>, car: Bytes) {
        let Some(peer) = offered_by.filter(|_| self.shinzo.is_some() && !car.is_empty()) else {
            return;
        };
        match Cid::try_from(cid) {
            Ok(head) => {
                if !self
                    .shinzo_inline
                    .offer(head, &peer.to_string(), car, Instant::now())
                {
                    debug!(%head, "Inline CAR not kept: wrong root or over budget");
                }
            }
            Err(error) => debug!(%error, "Inline CAR with an unreadable head"),
        }
    }

    /// Answer a CAR request for `root` to `peer_id` from an inline CAR that
    /// peer offered, if one is kept:
    /// the CAR fetch response is raised here, with no network request.
    /// Hands `response` back when there is none.
    pub(crate) fn answer_from_inline_car(
        &mut self,
        peer_id: PeerId,
        root: Cid,
        response: tokio::sync::oneshot::Sender<Result<()>>,
    ) -> Option<tokio::sync::oneshot::Sender<Result<()>>> {
        let Some(car) = self.shinzo_inline.take(&root, &peer_id.to_string()) else {
            return Some(response);
        };
        let events = self.event_tx.clone();
        self.spawned_tasks.spawn(async move {
            let event = HostEvent::CarFetchResponse {
                peer_id,
                root_cid: root,
                car_data: car.to_vec(),
            };
            if events.send(event).await.is_err() {
                debug!(%root, "Inline CAR response dropped: the receiver is gone");
            }
            let _ = response.send(Ok(()));
        });
        None
    }

    /// Batch a collection-topic update as Go Shinzo does, when the Shinzo
    /// wire is on and the topic has subscribers. `None` leaves the publish
    /// to the caller, so a topic with no subscribers reports it as before.
    pub(crate) fn batch_publish(
        &mut self,
        topic: &DefraTopic,
        message: &PushLogBroadcast,
    ) -> Option<Result<MessageId>> {
        let DefraTopic::Collection(collection) = topic else {
            return None;
        };
        self.shinzo.as_ref()?;
        let hash = topic.to_ident_topic().hash();
        if self.swarm.behaviour().topic_peers(&hash).is_empty() {
            return None;
        }
        let document = DocumentInfo {
            doc_id: message.doc_id.clone(),
            cid: message.cid.clone(),
            block: message.block.clone(),
            car: Bytes::new(),
        };
        if let Some(full) = self
            .shinzo_batcher
            .add(collection, document, Instant::now())
        {
            self.publish_batch(full);
        }
        Some(Ok(MessageId::new(message.cid.as_ref())))
    }

    /// Publish the batches that are due, or every batch when `all`.
    pub(super) fn flush_shinzo_batches(&mut self, all: bool) {
        let batches = match all {
            true => self.shinzo_batcher.drain(),
            false => self.shinzo_batcher.due(Instant::now()),
        };
        for batch in batches {
            self.publish_batch(batch);
        }
    }

    /// Publish a batch. A message whose publish fails for a reason that can
    /// pass (no subscribed peer yet, full send queues) is kept to try again,
    /// as a single update's gossip is retried; a batch that will never
    /// encode or publish, or has failed too often, is dropped with a warning,
    /// and its documents reach peers through replicators and document sync.
    fn publish_batch(&mut self, batch: Batch) {
        let creator = self.swarm.local_peer_id().to_string();
        let Batch {
            topic,
            documents,
            failures,
        } = batch;
        let count = documents.len();
        let messages = match encode_batch(&topic, &creator, documents) {
            Ok(messages) => messages,
            Err(error) => {
                warn!(%topic, %error, count, "Failed to encode a Go Shinzo batch");
                return;
            }
        };
        let ident = DefraTopic::Collection(topic.clone()).to_ident_topic();
        for (data, documents) in messages {
            let error = match self.swarm.behaviour_mut().publish(ident.clone(), data) {
                Ok(_) | Err(PublishError::Duplicate) => continue,
                Err(error) => error,
            };
            let retryable = matches!(
                error,
                PublishError::NoPeersSubscribedToTopic | PublishError::AllQueuesFull(_)
            );
            let count = documents.len();
            let failed = Batch {
                topic: topic.clone(),
                documents,
                failures,
            };
            if retryable && self.shinzo_batcher.retry(failed, Instant::now()) {
                debug!(%topic, ?error, count, failures, "Go Shinzo batch not published; will retry");
            } else {
                warn!(%topic, ?error, count, failures, "Dropped a Go Shinzo batch that could not be published");
            }
        }
    }
}
