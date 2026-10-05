//! Bounded channel between the tasks that send bitswap traffic and the behaviour that owns the swarm side.
//!
//! Invariant: the channel holds at most 1024 events; senders wait for room and every
//! public operation carries a deadline, so a stalled behaviour cannot park a task forever.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use kovan_channel::bounded;
use libp2p::PeerId;
use thiserror::Error;
use tokio::sync::oneshot;
use tracing::{debug, trace};

use crate::message::BitswapMessage;
use crate::protocol::ProtocolId;

const OUT_EVENT_CAPACITY: usize = 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_SEND_TIMEOUT: Duration = Duration::from_secs(3 * 60 + 5);
const MIN_SEND_TIMEOUT: Duration = Duration::from_secs(2);
const SEND_LATENCY: Duration = Duration::from_secs(2);
/// 100 kbit/s.
const MIN_SEND_RATE: u64 = (100 * 1000) / 8;

/// Outcome a dial waiter receives: the negotiated protocol when known.
pub type DialResult = Result<Option<ProtocolId>, String>;

/// A request from a network handle to the behaviour.
#[derive(Debug)]
pub enum OutEvent {
    /// Make sure the peer is connected.
    Dial {
        /// Peer to dial.
        peer: PeerId,
        /// Receives the dial outcome.
        response: oneshot::Sender<DialResult>,
        /// Dial id for logs.
        id: usize,
    },
    /// Send a message over a fresh substream.
    SendMessage {
        /// Destination.
        peer: PeerId,
        /// The message.
        message: BitswapMessage,
        /// Receives the send outcome.
        response: oneshot::Sender<Result<(), SendError>>,
    },
    /// Keep the connection to the peer alive.
    Protect {
        /// Peer to protect.
        peer: PeerId,
    },
    /// Stop keeping the connection alive.
    Unprotect {
        /// Peer to unprotect.
        peer: PeerId,
        /// Receives whether a protected connection existed.
        response: oneshot::Sender<bool>,
    },
}

/// Why a single send attempt failed.
#[derive(Debug, Clone, Error)]
pub enum SendError {
    /// The peer does not speak bitswap.
    #[error("protocol not supported")]
    ProtocolNotSupported,
    /// Any other failure.
    #[error("{0}")]
    Other(String),
}

/// Failures of the network handle's operations.
#[derive(Debug, Error)]
pub enum NetworkError {
    /// The dial was refused or failed.
    #[error("dial:{id} failed: {reason}")]
    DialFailed {
        /// Dial id.
        id: usize,
        /// Reason reported by the behaviour.
        reason: String,
    },
    /// The dial did not finish in time.
    #[error("dial:{0} timed out")]
    DialTimeout(usize),
    /// The behaviour dropped the dial request.
    #[error("dial:{0} abandoned")]
    DialAbandoned(usize),
    /// The send did not finish in time.
    #[error("send:{0} timed out")]
    SendTimeout(PeerId),
    /// The behaviour dropped the send request.
    #[error("send:{0} abandoned")]
    SendAbandoned(PeerId),
    /// The peer does not speak bitswap.
    #[error("send:{0} protocol not supported")]
    ProtocolNotSupported(PeerId),
    /// Every attempt failed.
    #[error("send:{peer} failed: {errors:?}")]
    SendFailed {
        /// Destination.
        peer: PeerId,
        /// One error per attempt.
        errors: Vec<SendError>,
    },
}

/// Cloneable handle that queues work for the behaviour.
#[derive(Clone)]
pub struct Network {
    sender: bounded::Sender<OutEvent>,
    self_id: PeerId,
    dial_id: Arc<AtomicUsize>,
}

impl fmt::Debug for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Network")
            .field("self_id", &self.self_id)
            .finish_non_exhaustive()
    }
}

type RecvFuture = Pin<Box<dyn Future<Output = Option<OutEvent>> + Send>>;

/// The behaviour's end of the network channel.
pub struct OutEvents {
    receiver: bounded::Receiver<OutEvent>,
    pending: Option<RecvFuture>,
}

impl fmt::Debug for OutEvents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutEvents").finish_non_exhaustive()
    }
}

impl OutEvents {
    /// Polls for the next request; pending forever once every handle is gone.
    pub fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<OutEvent> {
        let receiver = &self.receiver;
        let pending = self.pending.get_or_insert_with(|| {
            let receiver = receiver.clone();
            Box::pin(async move { receiver.recv_async().await })
        });
        match pending.as_mut().poll(cx) {
            Poll::Ready(event) => {
                self.pending = None;
                match event {
                    Some(event) => Poll::Ready(event),
                    None => Poll::Pending,
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Network {
    /// Creates a handle and the behaviour's receiving end.
    pub fn new(self_id: PeerId) -> (Network, OutEvents) {
        let (sender, receiver) = kovan_channel::bounded(OUT_EVENT_CAPACITY);
        let network = Network {
            sender,
            self_id,
            dial_id: Arc::new(AtomicUsize::new(0)),
        };
        let events = OutEvents {
            receiver,
            pending: None,
        };
        (network, events)
    }

    /// The local peer id.
    pub fn self_id(&self) -> &PeerId {
        &self.self_id
    }

    /// Sends the message, retrying on failure, within `timeout` overall.
    pub async fn send_message_with_retry_and_timeout(
        &self,
        peer: PeerId,
        message: BitswapMessage,
        retries: usize,
        timeout: Duration,
        backoff: Duration,
    ) -> Result<(), NetworkError> {
        trace!(%peer, "send start: {:?}", message);
        tokio::time::timeout(timeout, async {
            let mut errors = Vec::new();
            for attempt in 1..=retries {
                debug!(%peer, attempt, retries, "send attempt");
                let (response, outcome) = oneshot::channel();
                self.sender
                    .send_async(OutEvent::SendMessage {
                        peer,
                        message: message.clone(),
                        response,
                    })
                    .await;

                match outcome.await {
                    Ok(Ok(())) => return Ok(()),
                    Ok(Err(SendError::ProtocolNotSupported)) => {
                        return Err(NetworkError::ProtocolNotSupported(peer));
                    }
                    Err(_) => return Err(NetworkError::SendAbandoned(peer)),
                    Ok(Err(other)) => {
                        debug!(%peer, attempt, retries, error = %other, "send attempt failed");
                        errors.push(other);
                        if attempt < retries {
                            tokio::time::sleep(backoff).await;
                        }
                    }
                }
            }
            Err(NetworkError::SendFailed { peer, errors })
        })
        .await
        .map_err(|_| NetworkError::SendTimeout(peer))?
    }

    /// Makes sure the peer is connected, returning the negotiated protocol when known.
    pub async fn dial(
        &self,
        peer: PeerId,
        timeout: Duration,
    ) -> Result<Option<ProtocolId>, NetworkError> {
        let id = self.dial_id.fetch_add(1, Ordering::Relaxed);
        debug!(id, %peer, "dial");
        tokio::time::timeout(timeout, async {
            let (response, outcome) = oneshot::channel();
            self.sender
                .send_async(OutEvent::Dial { peer, response, id })
                .await;
            outcome
                .await
                .map_err(|_| NetworkError::DialAbandoned(id))?
                .map_err(|reason| NetworkError::DialFailed { id, reason })
        })
        .await
        .map_err(|_| NetworkError::DialTimeout(id))?
    }

    /// Dials the peer, then sends one message with a size-derived timeout.
    pub async fn send_message(
        &self,
        peer: PeerId,
        message: BitswapMessage,
    ) -> Result<(), NetworkError> {
        self.dial(peer, CONNECT_TIMEOUT).await?;
        let timeout = send_timeout(message.encoded_len());
        self.send_message_with_retry_and_timeout(peer, message, 1, timeout, Duration::ZERO)
            .await
    }

    /// Keeps the connection to the peer alive until it is unprotected.
    pub async fn protect_peer(&self, peer: PeerId) {
        trace!(%peer, "protect");
        self.sender.send_async(OutEvent::Protect { peer }).await;
    }

    /// Releases a protection; true when the peer was protected.
    pub async fn unprotect_peer(&self, peer: PeerId) -> bool {
        trace!(%peer, "unprotect");
        let (response, outcome) = oneshot::channel();
        self.sender
            .send_async(OutEvent::Unprotect { peer, response })
            .await;
        outcome.await.unwrap_or_default()
    }
}

/// An appropriate send timeout for a message of `size` bytes.
pub fn send_timeout(size: usize) -> Duration {
    let transfer = Duration::from_secs(size as u64 / MIN_SEND_RATE);
    (SEND_LATENCY + transfer).clamp(MIN_SEND_TIMEOUT, MAX_SEND_TIMEOUT)
}
