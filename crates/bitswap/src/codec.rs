//! Length-prefixed protobuf framing for bitswap messages.

use std::fmt;

use asynchronous_codec::{Decoder, Encoder};
use bytes::{Bytes, BytesMut};
use prost::Message;
use unsigned_varint::codec::UviBytes;

use crate::handler_error::BitswapHandlerError;
use crate::message::BitswapMessage;
use crate::protocol::ProtocolId;

/// Bitswap codec for the framing.
pub struct BitswapCodec {
    /// Codec for the unsigned varint length prefix of the frames.
    pub length_codec: UviBytes,
    /// Negotiated protocol, selects the message encoding.
    pub protocol: ProtocolId,
}

impl fmt::Debug for BitswapCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BitswapCodec")
            .field("length_codec", &"unsigned_varint::codec::UviBytes")
            .field("protocol", &self.protocol)
            .finish()
    }
}

impl BitswapCodec {
    /// A codec for the protocol with the given length codec.
    pub fn new(length_codec: UviBytes, protocol: ProtocolId) -> Self {
        BitswapCodec {
            length_codec,
            protocol,
        }
    }
}

impl Encoder for BitswapCodec {
    type Item<'a> = BitswapMessage;
    type Error = BitswapHandlerError;

    fn encode(&mut self, item: Self::Item<'_>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        tracing::trace!("sending message protocol: {:?}\n{:?}", self.protocol, item);

        let message = match self.protocol {
            ProtocolId::Legacy | ProtocolId::Bitswap100 => item.encode_as_proto_v0(),
            ProtocolId::Bitswap110 | ProtocolId::Bitswap120 => item.encode_as_proto_v1()?,
        };

        self.length_codec
            .encode(Bytes::from(message.encode_to_vec()), dst)
            .map_err(|_| BitswapHandlerError::MaxTransmissionSize)
    }
}

impl Decoder for BitswapCodec {
    type Item = (BitswapMessage, ProtocolId);
    type Error = BitswapHandlerError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let packet = match self.length_codec.decode(src).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                BitswapHandlerError::MaxTransmissionSize
            } else {
                BitswapHandlerError::Io(e)
            }
        })? {
            Some(p) => p,
            None => return Ok(None),
        };

        let message = BitswapMessage::try_from(packet.freeze())?;
        Ok(Some((message, self.protocol)))
    }
}
