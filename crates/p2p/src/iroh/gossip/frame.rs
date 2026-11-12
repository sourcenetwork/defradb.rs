//! Gossip frames on a stream: a four-byte big-endian length, then that many
//! bytes of a postcard-encoded protocol message.
//!
//! Proved to read back as written, to refuse a frame over the limit before
//! reading it, in `proofs/lean/GossipFrame/Framing.lean`; and to hold every
//! message iroh-gossip emits, in `proofs/lean/GossipFrame/Budget.lean`.

use iroh::endpoint::RecvStream;
use iroh::EndpointId;
use iroh_gossip::proto;

use crate::error::{Error, Result};

pub(super) type ProtoMessage = proto::Message<EndpointId>;

/// The largest frame we write or read. Every release so far reads frames up
/// to this size, so it never grows.
pub(super) const FRAME_LIMIT: usize = proto::DEFAULT_MAX_MESSAGE_SIZE;

/// A topic ID's encoding. iroh-gossip's own wire sends the topic once, in a
/// stream header, so its budget leaves it out; our frame carries it in every
/// message.
pub(super) const TOPIC_ID_LEN: usize = 32;

/// iroh-gossip's `max_message_size`. It keeps every message's tag and body
/// under it and sizes broadcasts against it, so with the topic added every
/// frame it emits fits `FRAME_LIMIT`.
pub(super) const MAX_MESSAGE_SIZE: usize = FRAME_LIMIT - TOPIC_ID_LEN;

/// What a swarm broadcast adds to its content in iroh-gossip's size check,
/// besides the content's length prefix: the message tag, the 32-byte message
/// ID, and the scope at its largest round (`plumtree::max_gossip_size`).
const BROADCAST_OVERHEAD: usize = 37;

/// The largest content iroh-gossip broadcasts; it drops anything larger.
pub(crate) const MAX_BROADCAST: usize = max_broadcast(MAX_MESSAGE_SIZE);

/// The bytes postcard's varint length prefix takes for `n`.
const fn varint_len(mut n: usize) -> usize {
    let mut len = 1;
    while n >= 0x80 {
        n >>= 7;
        len += 1;
    }
    len
}

/// The largest content whose broadcast fits iroh-gossip's budget of
/// `max_message_size - 2` (`topic::max_plumtree_size`).
const fn max_broadcast(max_message_size: usize) -> usize {
    let budget = max_message_size - 2 - BROADCAST_OVERHEAD;
    let mut n = budget;
    while n > 0 && n + varint_len(n) > budget {
        n -= 1;
    }
    n
}

/// `message` as a frame, or an error when it is over `limit`.
pub(super) fn encode_gossip_frame(message: &ProtoMessage, limit: usize) -> Result<Vec<u8>> {
    let body = postcard::to_stdvec(message).map_err(|e| Error::Codec(e.to_string()))?;
    if body.len() > limit {
        return Err(Error::Codec(format!(
            "gossip frame of {} bytes is over the {limit} byte limit",
            body.len()
        )));
    }
    let len = u32::try_from(body.len()).map_err(|e| Error::Codec(e.to_string()))?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

#[cfg(test)]
pub(super) async fn write_gossip_frame(
    send: &mut iroh::endpoint::SendStream,
    message: &ProtoMessage,
    limit: usize,
) -> Result<()> {
    let frame = encode_gossip_frame(message, limit)?;
    send.write_all(&frame)
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
