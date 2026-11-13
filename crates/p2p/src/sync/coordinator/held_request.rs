//! A pubsub_rpc request held open until its deadline.
//!
//! Gossipsub learns which topics a peer reads from that peer's SUBSCRIBE
//! control message, which arrives after the connection. A request published
//! in between has no topic peer, and rust-libp2p refuses it with
//! `InsufficientPeers`. Go's `PublishToTopic` (go-p2p, `WithRepublishing`)
//! keeps the request and sends it again to every peer that joins the topic
//! until its context ends; [`HeldRequest`] does the same, woken by the
//! topic's subscription [`Notify`] rather than by a timer. Nothing is sent
//! again once the deadline has passed, and a publish that has not finished
//! by then is abandoned.
//!
//! Gossipsub cannot address one peer, so a resend reaches every topic peer,
//! not only the one that joined: a peer that already answered runs its
//! handler again, and its second reply is dropped here. That repeat work on
//! responders is expected, and bounded by the deadline.

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
            if std::mem::take(&mut self.publish_due) && !self.publish_by_deadline().await? {
                return self.finish();
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
                // A join and the deadline can be ready together; the join
                // wins the select, so the deadline is checked here.
                Wake::Joined if Instant::now() >= self.deadline => return self.finish(),
                Wake::Joined => {
                    self.next_join = Box::pin(self.joined.notified());
                    self.publish_due = true;
                }
                Wake::Done => return self.finish(),
            }
        }
    }

    /// Sends the request once, for a caller that waits for no reply. A
    /// refused or unfinished send is an error, as no peer got the request.
    pub(super) async fn send(mut self) -> Result<()> {
        self.publish_by_deadline().await?;
        self.finish().map(drop)
    }

    /// `Ok(None)` once any send was accepted; otherwise the refusal, or a
    /// publish that did not finish before the deadline.
    fn finish(&mut self) -> Result<Option<PubsubResponse>> {
        if self.sent {
            return Ok(None);
        }
        Err(self.refused.take().unwrap_or_else(|| {
            Error::GossipSubPublish("publish did not complete before the deadline".to_string())
        }))
    }

    /// Publishes, unless the deadline passes first: `false` if it did.
    async fn publish_by_deadline(&mut self) -> Result<bool> {
        let deadline = self.deadline;
        let expired = async {
            crate::rt::time::sleep_until(deadline).await;
            Ok(false)
        };
        async { self.publish().await.map(|()| true) }
            .or(expired)
            .await
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
