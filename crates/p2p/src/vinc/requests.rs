//! The streams this node opens: requests and their replies, and CAR fetches.

use std::rc::Rc;
use std::time::Duration;

use cid::Cid;
use futures::FutureExt;
use tracing::{debug, warn};
use vinc::{Connection, Hosted, RecvStream, SendStream};

use crate::error::{Error, Result};
use crate::message::CarFetchRequest;
use crate::transport::{FetchFailureCause, PeerId, TransportEvent};
use crate::QueryId;

use super::addr::node_id;
use super::command::{Outbound, Reply};
use super::endpoint::Node;
use super::services::{self, MAX_CAR_SIZE};
use super::wire::{self, vinc_error};

const OPEN_STREAM_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a reply may take once the request is written.
pub(super) const REQUEST_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a one-way message waits for the peer to end its side.
const TELL_WAIT: Duration = Duration::from_secs(5);

/// How one provider's CAR fetch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CarOutcome {
    Success,
    SizeLimited,
    Busy,
    HeaderOnly,
    Empty,
    TimedOut,
    Failed,
}

impl CarOutcome {
    fn of(car: &[u8]) -> Self {
        if car.is_empty() {
            Self::Empty
        } else if crate::sync::car::is_car_busy(car) {
            Self::Busy
        } else if crate::sync::car::decode_car_oversized(car)
            .is_ok_and(|notices| !notices.is_empty())
        {
            Self::SizeLimited
        } else if crate::sync::car::car_has_any_block(car) {
            Self::Success
        } else {
            Self::HeaderOnly
        }
    }

    /// A shed serve answers busy; an older peer answers the same refusal empty.
    fn cause(self) -> FetchFailureCause {
        match self {
            Self::TimedOut => FetchFailureCause::TimedOut,
            Self::Empty | Self::Busy => FetchFailureCause::Refused,
            Self::HeaderOnly => FetchFailureCause::NoBlocks,
            Self::Success | Self::SizeLimited | Self::Failed => FetchFailureCause::Error,
        }
    }
}

impl<H: Hosted + 'static> Node<H> {
    /// Open a `tag` stream to `peer_id`, dialing it if no connection is held,
    /// write `body` and end the request.
    async fn open_request(
        self: &Rc<Self>,
        peer_id: &PeerId,
        tag: &'static [u8],
        body: &[u8],
    ) -> Result<(Connection<H>, SendStream<H>, RecvStream<H>)> {
        let peer = node_id(peer_id)?;
        let connection = self.connection(&peer, Vec::new()).await?;
        let (mut send, recv) =
            crate::rt::time::timeout(OPEN_STREAM_TIMEOUT, connection.open_bi_tagged(tag))
                .await
                .map_err(|_| Error::StreamOpen {
                    peer_id: peer_id.to_string(),
                    protocol: String::from_utf8_lossy(tag).into_owned(),
                    reason: "timed out opening a vinc stream".to_owned(),
                })?
                .map_err(|e| vinc_error("open stream", e))?;
        send.set_urgency(services::urgency_of(tag))
            .await
            .map_err(|e| vinc_error("stream urgency", e))?;
        wire::write_payload(&mut send, body).await?;
        send.finish()
            .await
            .map_err(|e| vinc_error("finish request", e))?;
        Ok((connection, send, recv))
    }

    /// Run one outbound request and answer `reply` as `outbound` says.
    pub(super) async fn request(
        self: Rc<Self>,
        peer_id: PeerId,
        tag: &'static [u8],
        body: Vec<u8>,
        outbound: Outbound,
        reply: Reply<Vec<u8>>,
    ) {
        let max_reply = services::lookup(tag).map_or(services::MAX_MESSAGE_SIZE, |s| s.max_message);
        let opened = self.open_request(&peer_id, tag, &body).await;
        drop(body);
        let (connection, _send, mut recv) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        match outbound {
            Outbound::Tell => {
                // Keep the connection until the peer has read the message.
                let _ = crate::rt::time::timeout(TELL_WAIT, recv.read_to_end(16)).await;
                let _ = reply.send(Ok(Vec::new()));
            }
            Outbound::Ask => {
                let _ = reply.send(read_reply_bytes(&connection, &mut recv, max_reply).await);
            }
            Outbound::Event {
                to_event,
                answer_when_sent,
            } => {
                let mut reply = Some(reply);
                if answer_when_sent {
                    if let Some(reply) = reply.take() {
                        let _ = reply.send(Ok(Vec::new()));
                    }
                }
                let event = read_reply_bytes(&connection, &mut recv, max_reply)
                    .await
                    .and_then(|bytes| to_event(peer_id.clone(), bytes));
                match event {
                    Ok(event) => {
                        if self.events.send(event).await.is_err() {
                            debug!("Event channel closed, cannot emit a vinc reply");
                        }
                        if let Some(reply) = reply.take() {
                            let _ = reply.send(Ok(Vec::new()));
                        }
                    }
                    Err(error) => {
                        debug!(%peer_id, tag = %String::from_utf8_lossy(tag), %error, "no vinc reply on the request stream");
                        if let Some(reply) = reply.take() {
                            let _ = reply.send(Err(error));
                        }
                    }
                }
            }
        }
    }

    /// Fetch the whole DAG below `root_cid` from `peer_id` and hand the CAR
    /// to the engine as a response with no query.
    pub(super) async fn car_request(
        self: &Rc<Self>,
        peer_id: &PeerId,
        root_cid: Cid,
    ) -> Result<()> {
        let request = CarFetchRequest::full_dag(root_cid);
        let car = self.fetch_car(peer_id, &request).await?;
        if self
            .events
            .send(TransportEvent::CarFetchResponse {
                query_id: None,
                peer_id: peer_id.clone(),
                root_cid,
                car_data: car,
            })
            .await
            .is_err()
        {
            warn!("Event channel closed, cannot emit CarFetchResponse");
        }
        Ok(())
    }

    async fn fetch_car(
        self: &Rc<Self>,
        peer_id: &PeerId,
        request: &CarFetchRequest,
    ) -> Result<Vec<u8>> {
        let (connection, _send, mut recv) = self
            .open_request(peer_id, services::STREAM_CAR, &wire::encode(request)?)
            .await?;
        let read = wire::read_to_end_observed(&mut recv, MAX_CAR_SIZE, |_| {});
        futures::select_biased! {
            car = crate::rt::time::timeout(REQUEST_RESPONSE_TIMEOUT, read).fuse() => {
                car.map_err(|_| Error::ResponseTimeout)?
            }
            reason = connection.closed().fuse() => Err(vinc_error("CAR fetch", reason)),
        }
    }

    /// Fetch `request` from every provider at once. The first reply that
    /// carries blocks ends the others; its completion comes from the engine
    /// once the blocks are stored. With none, a failure completes the query.
    pub(super) async fn block_sync(
        self: Rc<Self>,
        query_id: QueryId,
        request: CarFetchRequest,
        providers: Vec<PeerId>,
    ) {
        let mut attempts = futures::stream::FuturesUnordered::new();
        for provider in &providers {
            let node = Rc::clone(&self);
            let request = request.clone();
            let provider = provider.clone();
            attempts.push(async move {
                let outcome = match node.fetch_car(&provider, &request).await {
                    Ok(car) => {
                        let outcome = CarOutcome::of(&car);
                        if node
                            .events
                            .send(TransportEvent::CarFetchResponse {
                                query_id: Some(query_id),
                                peer_id: provider.clone(),
                                root_cid: request.root_cid,
                                car_data: car,
                            })
                            .await
                            .is_err()
                        {
                            warn!("Event channel closed, cannot emit CarFetchResponse");
                        }
                        outcome
                    }
                    Err(Error::ResponseTimeout) => CarOutcome::TimedOut,
                    Err(error) => {
                        debug!(%provider, %error, "vinc CAR fetch failed");
                        CarOutcome::Failed
                    }
                };
                (provider, outcome)
            });
        }
        let mut failures = Vec::new();
        let mut any_success = false;
        let mut size_limited = false;
        while let Some((provider, outcome)) = futures::StreamExt::next(&mut attempts).await {
            match outcome {
                CarOutcome::Success => {
                    any_success = true;
                    break;
                }
                CarOutcome::SizeLimited => {
                    size_limited = true;
                    failures.push((provider, outcome));
                }
                _ => failures.push((provider, outcome)),
            }
        }
        drop(attempts);
        if any_success || size_limited {
            return;
        }
        let cause = failures
            .iter()
            .map(|(_, outcome)| outcome.cause())
            .max()
            .unwrap_or(FetchFailureCause::Error);
        let busy = !failures.is_empty() && failures.iter().all(|(_, o)| *o == CarOutcome::Busy);
        let error = format!(
            "CAR fetch failed: {} provider(s) tried, none returned usable blocks (root={}); outcomes=[{}]",
            providers.len(),
            request.root_cid,
            failures
                .iter()
                .map(|(provider, outcome)| format!("{provider}:{outcome:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        if self
            .events
            .send(TransportEvent::BitswapComplete {
                query_id,
                success: false,
                cause: Some(cause),
                error: Some(error),
                provider_busy: busy,
            })
            .await
            .is_err()
        {
            warn!("Event channel closed, cannot emit BitswapComplete");
        }
    }
}

/// The reply's bytes, read within the deadline and before the connection
/// ends.
async fn read_reply_bytes<H>(
    connection: &Connection<H>,
    recv: &mut RecvStream<H>,
    max_reply: usize,
) -> Result<Vec<u8>>
where
    H: Hosted,
{
    let read = async {
        let len = wire::read_length(recv).await?;
        if len > max_reply {
            recv.stop(super::serve::RESET_MESSAGE_TOO_LARGE).await;
            return Err(Error::Codec(format!(
                "reply too large: {len} bytes exceeds limit of {max_reply} bytes"
            )));
        }
        let mut body = vec![0u8; len];
        recv.read_exact(&mut body)
            .await
            .map_err(|e| vinc_error("read reply", e))?;
        Ok(body)
    };
    futures::select_biased! {
        bytes = crate::rt::time::timeout(REQUEST_RESPONSE_TIMEOUT, read).fuse() => {
            bytes.map_err(|_| Error::ResponseTimeout)?
        }
        reason = connection.closed().fuse() => Err(vinc_error("reply", reason)),
    }
}
