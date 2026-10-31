//! Length-prefixed protobuf framing for bitswap messages.

use std::fmt;

use asynchronous_codec::{Decoder, Encoder};
use bytes::{Buf, Bytes, BytesMut};
use prost::Message;
use unsigned_varint::codec::UviBytes;

use crate::handler_error::BitswapHandlerError;
use crate::message::BitswapMessage;
use crate::protocol::ProtocolId;

/// Bitswap codec for the framing.
pub struct BitswapCodec {
    /// Codec for the unsigned varint length prefix of sent frames; its
    /// maximum length also bounds received frames.
    pub length_codec: UviBytes,
    /// Negotiated protocol, selects the message encoding.
    pub protocol: ProtocolId,
    /// Where the decoder stands between frames.
    reading: Reading,
}

/// The decoder's position in the received byte stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// At a frame's length prefix.
    Prefix,
    /// Inside the body of a frame of this length, not yet all received.
    Body(usize),
    /// Inside the body of a rejected oversized frame, this many bytes still
    /// to discard.
    Discard(usize),
}

impl fmt::Debug for BitswapCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BitswapCodec")
            .field("length_codec", &"unsigned_varint::codec::UviBytes")
            .field("protocol", &self.protocol)
            .field("reading", &self.reading)
            .finish()
    }
}

impl BitswapCodec {
    /// A codec for the protocol with the given length codec.
    pub fn new(length_codec: UviBytes, protocol: ProtocolId) -> Self {
        BitswapCodec {
            length_codec,
            protocol,
            reading: Reading::Prefix,
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

    /// A frame longer than the maximum is rejected once, and its body is
    /// discarded as it arrives, so the next frame on the stream is read from
    /// its own length prefix. Left in the buffer, the body would be read as
    /// frames: a zero-filled body as one empty message per byte.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Reading::Discard(left) = self.reading {
            let discarded = left.min(src.len());
            src.advance(discarded);
            if discarded < left {
                self.reading = Reading::Discard(left - discarded);
                return Ok(None);
            }
            self.reading = Reading::Prefix;
        }
        let len = match self.reading {
            Reading::Body(len) => len,
            _ => {
                let (len, rest) = match unsigned_varint::decode::usize(src) {
                    Ok(decoded) => decoded,
                    Err(unsigned_varint::decode::Error::Insufficient) => return Ok(None),
                    Err(error) => {
                        return Err(BitswapHandlerError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            error,
                        )))
                    }
                };
                src.advance(src.len() - rest.len());
                if len > self.length_codec.max_len() {
                    let discarded = len.min(src.len());
                    src.advance(discarded);
                    self.reading = Reading::Discard(len - discarded);
                    return Err(BitswapHandlerError::MaxTransmissionSize);
                }
                len
            }
        };
        if src.len() < len {
            src.reserve(len - src.len());
            self.reading = Reading::Body(len);
            return Ok(None);
        }
        self.reading = Reading::Prefix;
        let message = BitswapMessage::try_from(src.split_to(len).freeze())?;
        Ok(Some((message, self.protocol)))
    }
}
