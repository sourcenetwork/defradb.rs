//! Connection handler: one fresh outbound substream per message, inbound substreams read until EOF.
//!
//! Keep-alive: 30 s from connection start, reset to the idle timeout on every send and receive,
//! held forever by `Protect` and reset to 30 s by `Unprotect`.

use std::collections::VecDeque;
use std::fmt::Debug;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use asynchronous_codec::Framed;
use futures::prelude::*;
use futures::stream::{self, BoxStream, SelectAll};
use libp2p::core::upgrade::NegotiationError;
use libp2p::swarm::handler::{
    ConnectionEvent, DialUpgradeError, FullyNegotiatedInbound, FullyNegotiatedOutbound,
};
use libp2p::swarm::{
    ConnectionHandler, ConnectionHandlerEvent, Stream, StreamUpgradeError, SubstreamProtocol,
};
use tokio::sync::oneshot;
use tracing::{debug, trace};

use crate::codec::BitswapCodec;
use crate::handler_error::BitswapHandlerError;
use crate::message::BitswapMessage;
use crate::network::SendError;
use crate::protocol::{ProtocolConfig, ProtocolId};

const INITIAL_KEEP_ALIVE: Duration = Duration::from_secs(30);

/// An event the handler reports to the behaviour.
#[derive(Debug)]
pub enum HandlerEvent {
    /// A bitswap message was received.
    Message {
        /// The message.
        message: BitswapMessage,
        /// The protocol it arrived on.
        protocol: ProtocolId,
    },
    /// A substream failed.
    FailedToSendMessage {
        /// The failure.
        error: BitswapHandlerError,
    },
}

/// Receives the outcome of one send.
pub type SendResponse = oneshot::Sender<Result<(), SendError>>;

/// A command from the behaviour to the handler.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum BitswapHandlerIn {
    /// A message to send on a new substream.
    Message(BitswapMessage, SendResponse),
    /// Keep the connection alive indefinitely.
    Protect,
    /// Resume the normal keep-alive schedule.
    Unprotect,
}

type HandlerEvents =
    ConnectionHandlerEvent<ProtocolConfig, (BitswapMessage, SendResponse), HandlerEvent>;
type Substream = Framed<Stream, BitswapCodec>;

/// Handler for the bitswap substreams of one connection.
pub struct BitswapHandler {
    listen_protocol: SubstreamProtocol<ProtocolConfig, ()>,
    outbound_substreams: SelectAll<BoxStream<'static, HandlerEvents>>,
    inbound_substreams: SelectAll<BoxStream<'static, HandlerEvents>>,
    send_queue: VecDeque<(BitswapMessage, SendResponse)>,
    idle_timeout: Duration,
    upgrade_errors: VecDeque<StreamUpgradeError<BitswapHandlerError>>,
    keep_alive_until: Option<Instant>,
}

impl Debug for BitswapHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BitswapHandler")
            .field("outbound_substreams", &self.outbound_substreams.len())
            .field("inbound_substreams", &self.inbound_substreams.len())
            .field("send_queue", &self.send_queue.len())
            .field("idle_timeout", &self.idle_timeout)
            .field("keep_alive_until", &self.keep_alive_until)
            .finish()
    }
}

impl BitswapHandler {
    /// Builds a handler.
    pub fn new(protocol_config: ProtocolConfig, idle_timeout: Duration) -> Self {
        Self {
            listen_protocol: SubstreamProtocol::new(protocol_config, ()),
            outbound_substreams: Default::default(),
            inbound_substreams: Default::default(),
            send_queue: Default::default(),
            idle_timeout,
            upgrade_errors: VecDeque::new(),
            keep_alive_until: Some(Instant::now() + INITIAL_KEEP_ALIVE),
        }
    }

    fn on_fully_negotiated_inbound(
        &mut self,
        FullyNegotiatedInbound {
            protocol: substream,
            info: (),
        }: FullyNegotiatedInbound<ProtocolConfig, ()>,
    ) {
        trace!("new inbound substream: {:?}", substream.codec().protocol);
        self.inbound_substreams
            .push(Box::pin(inbound_substream(substream)));
    }

    fn on_fully_negotiated_outbound(
        &mut self,
        FullyNegotiatedOutbound {
            protocol: substream,
            info: message,
        }: FullyNegotiatedOutbound<ProtocolConfig, (BitswapMessage, SendResponse)>,
    ) {
        trace!("new outbound substream: {:?}", substream.codec().protocol);
        self.outbound_substreams
            .push(Box::pin(outbound_substream(substream, message)));
    }

    fn on_dial_upgrade_error(
        &mut self,
        DialUpgradeError { error, info: _ }: DialUpgradeError<
            (BitswapMessage, SendResponse),
            ProtocolConfig,
        >,
    ) {
        debug!("dial upgrade error {:?}", error);
        self.upgrade_errors.push_back(error);
    }
}

impl ConnectionHandler for BitswapHandler {
    type FromBehaviour = BitswapHandlerIn;
    type ToBehaviour = HandlerEvent;
    type InboundOpenInfo = ();
    type InboundProtocol = ProtocolConfig;
    type OutboundOpenInfo = (BitswapMessage, SendResponse);
    type OutboundProtocol = ProtocolConfig;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        self.listen_protocol.clone()
    }

    fn on_behaviour_event(&mut self, message: BitswapHandlerIn) {
        match message {
            BitswapHandlerIn::Message(message, response) => {
                self.send_queue.push_back((message, response));
                self.keep_alive_until = Some(Instant::now() + self.idle_timeout);
            }
            BitswapHandlerIn::Protect => self.keep_alive_until = None,
            BitswapHandlerIn::Unprotect => {
                self.keep_alive_until = Some(Instant::now() + INITIAL_KEEP_ALIVE);
            }
        }
    }

    fn connection_keep_alive(&self) -> bool {
        self.keep_alive_until
            .is_none_or(|until| Instant::now() < until)
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<HandlerEvents> {
        if let Some(error) = self.upgrade_errors.pop_front() {
            let error = match error {
                StreamUpgradeError::Timeout => BitswapHandlerError::NegotiationTimeout,
                StreamUpgradeError::Apply(e) => e,
                StreamUpgradeError::NegotiationFailed => {
                    BitswapHandlerError::NegotiationProtocolError(NegotiationError::Failed)
                }
                StreamUpgradeError::Io(e) => BitswapHandlerError::Io(e),
            };
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                HandlerEvent::FailedToSendMessage { error },
            ));
        }

        if let Some(message) = self.send_queue.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: self.listen_protocol.clone().map_info(|()| message),
            });
        }

        if let Poll::Ready(Some(event)) = self.outbound_substreams.poll_next_unpin(cx) {
            return Poll::Ready(event);
        }

        if let Poll::Ready(Some(event)) = self.inbound_substreams.poll_next_unpin(cx) {
            if let ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::Message { .. }) = event {
                self.keep_alive_until = Some(Instant::now() + self.idle_timeout);
            }
            return Poll::Ready(event);
        }

        Poll::Pending
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(event) => {
                self.on_fully_negotiated_inbound(event)
            }
            ConnectionEvent::FullyNegotiatedOutbound(event) => {
                self.on_fully_negotiated_outbound(event)
            }
            ConnectionEvent::DialUpgradeError(event) => self.on_dial_upgrade_error(event),
            _ => {}
        }
    }
}

async fn close(mut substream: Substream) {
    if let Err(err) = substream.flush().await {
        debug!("failed to flush stream: {:?}", err);
    }
    if let Err(err) = substream.close().await {
        debug!("failed to close stream: {:?}", err);
    }
}

enum Inbound {
    Reading(Substream),
    Closing(Substream),
}

fn inbound_substream(substream: Substream) -> impl futures::Stream<Item = HandlerEvents> {
    stream::unfold(Inbound::Reading(substream), |state| async move {
        let mut substream = match state {
            Inbound::Reading(substream) => substream,
            Inbound::Closing(substream) => {
                close(substream).await;
                return None;
            }
        };
        loop {
            match substream.next().await {
                Some(Ok((message, protocol))) => {
                    let event = HandlerEvent::Message { message, protocol };
                    return Some((
                        ConnectionHandlerEvent::NotifyBehaviour(event),
                        Inbound::Reading(substream),
                    ));
                }
                Some(Err(BitswapHandlerError::MaxTransmissionSize)) => {
                    debug!("message exceeded the maximum transmission size");
                }
                Some(Err(error)) => {
                    debug!("inbound stream error: {}", error);
                    let event = HandlerEvent::FailedToSendMessage { error };
                    return Some((
                        ConnectionHandlerEvent::NotifyBehaviour(event),
                        Inbound::Closing(substream),
                    ));
                }
                None => {
                    close(substream).await;
                    return None;
                }
            }
        }
    })
}

enum Outbound {
    Sending(Substream, BitswapMessage, SendResponse),
    Closing(Substream),
}

fn outbound_substream(
    substream: Substream,
    (message, response): (BitswapMessage, SendResponse),
) -> impl futures::Stream<Item = HandlerEvents> {
    stream::unfold(
        Outbound::Sending(substream, message, response),
        |state| async move {
            match state {
                Outbound::Sending(mut substream, message, response) => {
                    if let Err(error) = substream.feed(message).await {
                        debug!("failed to write bitswap message: {:?}", error);
                        response.send(Err(SendError::Other(error.to_string()))).ok();
                        let event = HandlerEvent::FailedToSendMessage { error };
                        return Some((
                            ConnectionHandlerEvent::NotifyBehaviour(event),
                            Outbound::Closing(substream),
                        ));
                    }
                    response.send(Ok(())).ok();
                    close(substream).await;
                    None
                }
                Outbound::Closing(substream) => {
                    close(substream).await;
                    None
                }
            }
        },
    )
}
