//! Opening a stream for an embedder protocol.

use std::rc::Rc;

use pinc::Hosted;

use p2p::error::{Error, Result};
use p2p::transport::{PeerAddr, PeerId};

use super::addr::{node_id, socket_addrs};
use super::endpoint::Node;
use super::protocol_stream::{bridge, ProtocolRecvStream, ProtocolSendStream};
use super::requests::OPEN_STREAM_TIMEOUT;
use super::stream_protocols::StreamProtocol;
use super::wire::{self, pinc_error};

impl<H: Hosted + 'static> Node<H> {
    /// Open a `protocol` stream to `peer_id`, dialing `addrs` if no
    /// connection is held, and write `request` as its first message. The
    /// halves' pumps run on this thread until each half is done.
    pub(super) async fn open_protocol_stream(
        self: &Rc<Self>,
        peer_id: &PeerId,
        addrs: Vec<PeerAddr>,
        protocol: StreamProtocol,
        request: Vec<u8>,
    ) -> Result<(ProtocolSendStream, ProtocolRecvStream)> {
        protocol.validate()?;
        if request.len() > protocol.max_message {
            return Err(Error::RequestTooLarge {
                tag: String::from_utf8_lossy(protocol.tag).into_owned(),
                len: request.len(),
                max: protocol.max_message,
            });
        }
        let peer = node_id(peer_id)?;
        let connection = self.connection(&peer, socket_addrs(&addrs)).await?;
        let (mut send, recv) =
            p2p::rt::time::timeout(OPEN_STREAM_TIMEOUT, connection.open_bi_tagged(protocol.tag))
                .await
                .map_err(|_| Error::StreamOpen {
                    peer_id: peer_id.to_string(),
                    protocol: String::from_utf8_lossy(protocol.tag).into_owned(),
                    reason: "timed out opening a pinc stream".to_owned(),
                })?
                .map_err(|e| pinc_error("open stream", e))?;
        send.set_urgency(protocol.priority.urgency())
            .await
            .map_err(|e| pinc_error("stream urgency", e))?;
        wire::write_payload(&mut send, &request).await?;
        let (send, recv, pump_send, pump_recv) = bridge(send, recv);
        if self.spawn(pump_send).is_none() || self.spawn(pump_recv).is_none() {
            return Err(Error::Transport("the pinc endpoint is busy".to_owned()));
        }
        Ok((send, recv))
    }
}
