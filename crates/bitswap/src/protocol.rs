//! Protocol ids and the libp2p upgrade that frames a negotiated substream.

use std::future::Future;
use std::pin::Pin;

use asynchronous_codec::Framed;
use futures::future;
use futures::io::{AsyncRead, AsyncWrite};
use libp2p::core::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::StreamProtocol;
use unsigned_varint::codec::UviBytes;

use crate::codec::BitswapCodec;
use crate::handler_error::BitswapHandlerError;

const MAX_BUF_SIZE: usize = 1024 * 1024 * 2;

/// A bitswap protocol version.
#[derive(Clone, Debug, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProtocolId {
    /// `/ipfs/bitswap`.
    Legacy = 0,
    /// `/ipfs/bitswap/1.0.0`.
    Bitswap100 = 1,
    /// `/ipfs/bitswap/1.1.0`.
    Bitswap110 = 2,
    /// `/ipfs/bitswap/1.2.0`.
    Bitswap120 = 3,
}

impl ProtocolId {
    /// The multistream protocol name.
    pub fn protocol_name(&self) -> &'static str {
        match self {
            ProtocolId::Legacy => "/ipfs/bitswap",
            ProtocolId::Bitswap100 => "/ipfs/bitswap/1.0.0",
            ProtocolId::Bitswap110 => "/ipfs/bitswap/1.1.0",
            ProtocolId::Bitswap120 => "/ipfs/bitswap/1.2.0",
        }
    }

    /// The name as a libp2p stream protocol.
    pub fn as_stream_protocol(&self) -> StreamProtocol {
        StreamProtocol::new(self.protocol_name())
    }

    /// Parses a protocol name.
    pub fn try_from_str(value: &str) -> Option<Self> {
        match value {
            "/ipfs/bitswap" => Some(ProtocolId::Legacy),
            "/ipfs/bitswap/1.0.0" => Some(ProtocolId::Bitswap100),
            "/ipfs/bitswap/1.1.0" => Some(ProtocolId::Bitswap110),
            "/ipfs/bitswap/1.2.0" => Some(ProtocolId::Bitswap120),
            _ => None,
        }
    }

    /// Parses a protocol name from bytes.
    pub fn try_from(value: impl AsRef<[u8]>) -> Option<Self> {
        std::str::from_utf8(value.as_ref())
            .ok()
            .and_then(Self::try_from_str)
    }

    /// Whether the protocol carries HAVE and DONT_HAVE.
    pub fn supports_have(self) -> bool {
        matches!(self, ProtocolId::Bitswap120)
    }
}

impl AsRef<str> for ProtocolId {
    fn as_ref(&self) -> &str {
        self.protocol_name()
    }
}

/// Upgrade configuration for the bitswap protocols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolConfig {
    /// The bitswap protocols to listen on, in preference order.
    pub protocol_ids: Vec<ProtocolId>,
    /// Maximum size of a packet.
    pub max_transmit_size: usize,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        ProtocolConfig {
            protocol_ids: vec![
                ProtocolId::Bitswap120,
                ProtocolId::Bitswap110,
                ProtocolId::Bitswap100,
                ProtocolId::Legacy,
            ],
            max_transmit_size: MAX_BUF_SIZE,
        }
    }
}

impl UpgradeInfo for ProtocolConfig {
    type Info = StreamProtocol;
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.protocol_ids
            .iter()
            .map(|p| p.as_stream_protocol())
            .collect()
    }
}

impl ProtocolConfig {
    fn frame<TSocket: AsyncRead + AsyncWrite>(
        &self,
        socket: TSocket,
        protocol_id: &StreamProtocol,
    ) -> Framed<TSocket, BitswapCodec> {
        let mut length_codec = UviBytes::default();
        length_codec.set_max_len(self.max_transmit_size);
        let protocol =
            ProtocolId::try_from_str(protocol_id.as_ref()).unwrap_or(ProtocolId::Bitswap120);
        Framed::new(socket, BitswapCodec::new(length_codec, protocol))
    }
}

type UpgradeFuture<T> = Pin<Box<dyn Future<Output = Result<T, BitswapHandlerError>> + Send>>;

impl<TSocket> InboundUpgrade<TSocket> for ProtocolConfig
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = Framed<TSocket, BitswapCodec>;
    type Error = BitswapHandlerError;
    type Future = UpgradeFuture<Self::Output>;

    fn upgrade_inbound(self, socket: TSocket, protocol_id: Self::Info) -> Self::Future {
        Box::pin(future::ok(self.frame(socket, &protocol_id)))
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for ProtocolConfig
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = Framed<TSocket, BitswapCodec>;
    type Error = BitswapHandlerError;
    type Future = UpgradeFuture<Self::Output>;

    fn upgrade_outbound(self, socket: TSocket, protocol_id: Self::Info) -> Self::Future {
        Box::pin(future::ok(self.frame(socket, &protocol_id)))
    }
}
