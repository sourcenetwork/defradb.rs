//! Gossip frames on a stream: a four-byte big-endian length, then that many
//! bytes of a postcard-encoded protocol message.
//!
//! Proved to read back as written, and to refuse a frame over the limit
//! before reading it, in `proofs/lean/GossipFrame/Framing.lean`.

use iroh::endpoint::{RecvStream, SendStream};
use iroh::EndpointId;
use iroh_gossip::proto;

use crate::error::{Error, Result};

pub(super) type ProtoMessage = proto::Message<EndpointId>;

pub(super) async fn write_gossip_frame(
    send: &mut SendStream,
    message: &ProtoMessage,
    limit: usize,
) -> Result<()> {
    let body = postcard::to_stdvec(message).map_err(|e| Error::Codec(e.to_string()))?;
    if body.len() > limit {
        return Err(Error::Codec(format!(
            "gossip frame of {} bytes is over the {limit} byte limit",
            body.len()
        )));
    }
    let len = u32::try_from(body.len()).map_err(|e| Error::Codec(e.to_string()))?;
    send.write_all(&len.to_be_bytes())
        .await
        .map_err(|e| Error::Transport(e.to_string()))?;
    send.write_all(&body)
        .await
        .map_err(|e| Error::Transport(e.to_string()))
}

/// The next frame, or `None` when the stream ends cleanly between frames.
pub(super) async fn read_gossip_frame(
    recv: &mut RecvStream,
    limit: usize,
) -> Result<Option<ProtoMessage>> {
    let mut len = [0u8; 4];
    match recv.read_exact(&mut len).await {
        Ok(()) => {}
        Err(iroh::endpoint::ReadExactError::FinishedEarly(0)) => return Ok(None),
        Err(e) => return Err(Error::Transport(e.to_string())),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > limit {
        return Err(Error::Codec(format!(
            "gossip frame of {len} bytes is over the {limit} byte limit"
        )));
    }
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body)
        .await
        .map_err(|e| Error::Transport(e.to_string()))?;
    postcard::from_bytes(&body)
        .map(Some)
        .map_err(|e| Error::Codec(e.to_string()))
}

#[cfg(test)]
#[path = "../../../tests/iroh/gossip_frame.rs"]
mod tests;
