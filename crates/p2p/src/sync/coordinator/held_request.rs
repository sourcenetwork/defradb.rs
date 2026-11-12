//! A pubsub_rpc request held open until its deadline.
//!
//! Gossipsub learns which topics a peer reads from that peer's SUBSCRIBE
//! control message, which arrives after the connection. A request published
//! in between has no topic peer, and rust-libp2p refuses it with
//! `InsufficientPeers`. Go's `PublishToTopic` (go-p2p, `WithRepublishing`)
//! keeps the request and sends it again to every peer that joins the topic
//! until its context ends; [`HeldRequest`] does the same, woken by the
//! topic's subscription [`Notify`] rather than by a timer.

use std::pin::Pin;

use futures_lite::FutureExt;
use rapidhash::{HashSetExt, RapidHashSet};
use regolith::sync::{Notified, Notify};
use tracing::warn;

use crate::error::{Error, Result};
use crate::pubsub_rpc::PubsubResponse;
use crate::rt::mpsc;
use crate::rt::time::Instant;
use crate::transport::P2PTransport;

/// What woke a [`HeldRequest`] that was waiting for a reply.
enum Wake {
    Reply(PubsubResponse),
    Joined,
    Done,
}

/// A request on `topic`, sent once a peer subscribes to it and again each
/// time another one does, until `deadline`. Each responder's first reply is
/// returned; a resend's duplicates are dropped.
pub(super) struct HeldRequest<'a, T> {
    transport: &'a T,
    topic: &'static str,
    data: Vec<u8>,
    joined: &'a Notify,
    next_join: Pin<Box<Notified<'a>>>,
    responses: &'a mut mpsc::Receiver<PubsubResponse>,
    deadline: Instant,
    publish_due: bool,
    sent: bool,
    refused: Option<Error>,
    answered: RapidHashSet<String>,
}

impl<'a, T> HeldRequest<'a, T>
where
    T: P2PTransport,
{
    /// `joined` must be notified whenever a peer subscribes to `topic`.
    pub(super) fn new(
        transport: &'a T,
        topic: &'static str,
        data: Vec<u8>,
        joined: &'a Notify,
        responses: &'a mut mpsc::Receiver<PubsubResponse>,
        deadline: Instant,
    ) -> Self {
        Self {
            transport,
            topic,
            data,
            joined,
            // Taken before the first publish, so a peer that subscribes while
            // it is refused still wakes the request.
            next_join: Box::pin(joined.notified()),
            responses,
            deadline,
            publish_due: true,
            sent: false,
            refused: None,
            answered: RapidHashSet::new(),
        }
    }

    /// The next new responder's reply, `None` at the deadline or once the
    /// replies close. A request no peer ever received ends with the publish
    /// error instead.
    pub(super) async fn next(&mut self) -> Result<Option<PubsubResponse>> {
        loop {
            if std::mem::take(&mut self.publish_due) {
                self.publish().await?;
            }
            let (responses, next_join, deadline) =
                (&mut *self.responses, self.next_join.as_mut(), self.deadline);
            let reply = async { responses.recv().await.map_or(Wake::Done, Wake::Reply) };
            let joined = async {
                next_join.await;
                Wake::Joined
            };
            let expired = async {
                crate::rt::time::sleep_until(deadline).await;
                Wake::Done
            };
            match reply.or(joined).or(expired).await {
                Wake::Reply(response) => {
                    if self.answered.insert(response.from.clone()) {
                        return Ok(Some(response));
                    }
                }
                Wake::Joined => {
                    self.next_join = Box::pin(self.joined.notified());
                    self.publish_due = true;
                }
                Wake::Done => {
                    return match self.refused.take() {
                        Some(error) if !self.sent => Err(error),
                        _ => Ok(None),
                    };
                }
            }
        }
    }

    async fn publish(&mut self) -> Result<()> {
        match self
            .transport
            .publish_raw(self.topic.to_string(), self.data.clone())
            .await
        {
            Ok(_) => self.sent = true,
            Err(error) if is_no_topic_peer(&error) => {
                if !self.sent {
                    self.refused = Some(error);
                }
            }
            Err(error) if !self.sent => {
                warn!(topic = self.topic, error = %error, "pubsub_rpc publish failed");
                return Err(error);
            }
            Err(error) => {
                warn!(topic = self.topic, error = %error, "pubsub_rpc resend failed");
            }
        }
        Ok(())
    }
}

fn is_no_topic_peer(error: &Error) -> bool {
    matches!(error, Error::GossipSubPublish(message) if message == "InsufficientPeers")
}
