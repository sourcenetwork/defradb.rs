//! The mux ALPN, its stream tags, and wire format helpers for iroh transport.
//!
//! The discriminator is in-band — a tag in the stream's first frame — because
//! ALPN is negotiated once per TLS handshake and so cannot vary per stream.

use iroh::endpoint::{RecvStream, SendStream};

/// The single ALPN every Defra protocol is multiplexed over, gossip included.
pub const ALPN_MUX: &[u8] = b"/defra-iroh/mux/0.2";

/// The mux before gossip moved onto it. Still accepted and offered, so older
/// peers keep every protocol except gossip.
pub const ALPN_MUX_V1: &[u8] = b"/defra-iroh/mux/0.1";

/// Stream tag for the gossip stream, the only uni stream on a connection.
pub const STREAM_GOSSIP: &[u8] = b"/defra-iroh/gossip/0.1";

/// Stream tag for PushLog request-response.
pub const STREAM_PUSHLOG: &[u8] = b"/defra-iroh/pushlog/0.1";

/// Stream tag for document sync request.
pub const STREAM_DOCSYNC: &[u8] = b"/defra-iroh/docsync/0.1";

/// Stream tag for document sync response (separate from request to avoid ambiguous decoding).
pub const STREAM_DOCSYNC_RESP: &[u8] = b"/defra-iroh/docsync/0.1/resp";

/// Stream tag for branchable sync request.
pub const STREAM_BRANCHABLE: &[u8] = b"/defra-iroh/branchable/0.1";

/// Stream tag for branchable sync response.
pub const STREAM_BRANCHABLE_RESP: &[u8] = b"/defra-iroh/branchable/0.1/resp";

/// Stream tag for CAR block transfer request.
pub const STREAM_CAR: &[u8] = b"/defra-iroh/car/0.1";

/// Stream tag for CAR block transfer response.
pub const STREAM_CAR_RESP: &[u8] = b"/defra-iroh/car/0.1/resp";

/// Stream tag for audience-bound Defra peer identity resolution.
pub const STREAM_IDENTITY: &[u8] = b"/defra-iroh/identity/0.1";

/// Stream tag for searchable encryption artifacts.
pub const STREAM_SE: &[u8] = b"/defra-iroh/se/0.1";

/// Stream tag for searchable encryption artifact query requests.
pub const STREAM_SE_QUERY_REQ: &[u8] = b"/defra-iroh/se-query/0.1/req";

/// Stream tag for searchable encryption artifact query responses.
pub const STREAM_SE_QUERY_RESP: &[u8] = b"/defra-iroh/se-query/0.1/resp";

/// Stream tag for management mutate requests.
pub const STREAM_MANAGE_REQ: &[u8] = b"/defra-iroh/manage/0.1/req";

/// Stream tag for management mutate responses.
pub const STREAM_MANAGE_RESP: &[u8] = b"/defra-iroh/manage/0.1/resp";

/// Stream tag for management query requests.
pub const STREAM_MANAGE_QUERY_REQ: &[u8] = b"/defra-iroh/manage-query/0.1/req";

/// Stream tag for management query responses.
pub const STREAM_MANAGE_QUERY_RESP: &[u8] = b"/defra-iroh/manage-query/0.1/resp";

/// Stream tag for service record requests and replies, both ways.
pub const STREAM_SERVICE_RECORDS: &[u8] = b"/defra-iroh/services/0.1";

/// Stream tag for the two-stream push protocol.
pub const STREAM_TWOSTREAM: &[u8] = b"/defra-iroh/twostream/0.1";

/// Stream tag for two-stream push replies.
pub const STREAM_TWOSTREAM_RESP: &[u8] = b"/defra-iroh/twostream/0.1/resp";

/// Upper bound on a stream tag, enforced on read so a peer cannot make us
/// allocate on a length it never intends to fill.
pub const MAX_STREAM_TAG_LEN: usize = 64;

/// Maximum size for general messages (matches libp2p default).
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

/// Maximum size for CAR block transfers (matches libp2p default).
pub const MAX_CAR_SIZE: usize = 64 * 1024 * 1024; // 64 MiB

/// Maximum size for management messages.
pub const MAX_MANAGE_MSG_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

/// A records reply: at most `MAX_REPLY_RECORDS` records with their signatures.
pub const MAX_RECORDS_MSG_SIZE: usize =
    super::records::MAX_REPLY_RECORDS * (super::records::MAX_RECORD_BYTES + 128);

/// Write the tag naming the protocol this stream carries.
///
/// The length is one byte to distinguish it from the four-byte message length
/// prefix that follows.
pub async fn write_stream_tag(send: &mut SendStream, tag: &[u8]) -> crate::error::Result<()> {
    if tag.is_empty() || tag.len() > MAX_STREAM_TAG_LEN {
        return Err(crate::error::Error::Codec(format!(
            "stream tag of {} bytes does not fit a tag frame",
            tag.len()
        )));
    }

    if let Some(service) = super::services::lookup(tag) {
        // Only fails on a stream that is already closed, which the write below reports.
        let _ = send.set_priority(service.priority.send_priority());
    }
    send.write_all(&[tag.len() as u8])
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to write tag length: {}", e)))?;
    send.write_all(tag)
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to write tag: {}", e)))?;
    Ok(())
}

/// Read the tag a peer wrote as the first frame of a mux stream.
pub async fn read_stream_tag(recv: &mut RecvStream) -> crate::error::Result<Vec<u8>> {
    let mut len_buf = [0u8; 1];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to read tag length: {}", e)))?;

    let len = len_buf[0] as usize;
    if len == 0 || len > MAX_STREAM_TAG_LEN {
        return Err(crate::error::Error::Codec(format!(
            "invalid stream tag length: {}",
            len
        )));
    }

    let mut tag = vec![0u8; len];
    recv.read_exact(&mut tag)
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to read tag: {}", e)))?;
    Ok(tag)
}

/// Read a length-prefixed CBOR message from a QUIC recv stream.
///
/// The `max_size` parameter caps the allocation to prevent a malicious peer
/// from sending a large length prefix and causing an OOM.
pub async fn read_message<T: serde::de::DeserializeOwned>(
    recv: &mut RecvStream,
    max_size: usize,
) -> crate::error::Result<T> {
    let len = read_length(recv).await?;
    if len > max_size {
        return Err(crate::error::Error::Codec(format!(
            "message too large: {} bytes exceeds limit of {} bytes",
            len, max_size
        )));
    }
    decode(read_body(recv, len).await?)
}

/// Read the four-byte length prefix of a message.
pub async fn read_length(recv: &mut RecvStream) -> crate::error::Result<usize> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to read length: {}", e)))?;
    Ok(u32::from_be_bytes(len_buf) as usize)
}

/// Read a message body whose length has already been read and admitted.
pub async fn read_body(recv: &mut RecvStream, len: usize) -> crate::error::Result<Vec<u8>> {
    let mut payload = vec![0u8; len];
    recv.read_exact(&mut payload)
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to read payload: {}", e)))?;
    Ok(payload)
}

/// Reads a stream to its end, as `RecvStream::read_to_end` does, calling
/// `on_chunk` with the size of each chunk as it arrives.
pub async fn read_to_end_observed<OnChunk>(
    recv: &mut RecvStream,
    size_limit: usize,
    mut on_chunk: OnChunk,
) -> crate::error::Result<Vec<u8>>
where
    OnChunk: FnMut(usize),
{
    // Chunks are kept and copied once into an exact-size buffer, as
    // `read_to_end` does, rather than regrowing one buffer per chunk.
    let mut chunks = Vec::new();
    let mut len = 0;
    while let Some(chunk) = recv
        .read_chunk(size_limit + 1 - len)
        .await
        .map_err(|e| crate::error::Error::Transport(e.to_string()))?
    {
        on_chunk(chunk.len());
        len += chunk.len();
        if len > size_limit {
            return Err(crate::error::Error::Transport(format!(
                "stream longer than {size_limit} bytes"
            )));
        }
        chunks.push(chunk);
    }
    Ok(chunks.concat())
}

/// Read a `len`-byte body, giving up once no bytes arrive for `idle`.
/// `Ok(None)` means the sender stalled.
pub async fn read_body_until_idle(
    recv: &mut RecvStream,
    len: usize,
    idle: std::time::Duration,
) -> crate::error::Result<Option<Vec<u8>>> {
    let mut payload = vec![0u8; len];
    let mut filled = 0;
    while filled < len {
        let Ok(read) = n0_future::time::timeout(idle, recv.read(&mut payload[filled..])).await
        else {
            return Ok(None);
        };
        match read
            .map_err(|e| crate::error::Error::Codec(format!("failed to read payload: {}", e)))?
        {
            Some(n) => filled += n,
            None => {
                return Err(crate::error::Error::Codec(format!(
                    "stream ended after {filled} of {len} payload bytes"
                )))
            }
        }
    }
    Ok(Some(payload))
}

/// Decode a CBOR message body. Passing the body by value frees it once
/// decoded, so it is not held twice while the message waits to be queued.
pub fn decode<T, Body>(payload: Body) -> crate::error::Result<T>
where
    T: serde::de::DeserializeOwned,
    Body: AsRef<[u8]>,
{
    defra_core::cbor::from_slice(payload.as_ref())
        .map_err(|e| crate::error::Error::Codec(e.to_string()))
}

/// Write a length-prefixed CBOR message to a QUIC send stream.
pub async fn write_message<T: serde::Serialize>(
    send: &mut SendStream,
    value: &T,
) -> crate::error::Result<()> {
    let payload =
        defra_core::cbor::to_vec(value).map_err(|e| crate::error::Error::Codec(e.to_string()))?;
    let len = (payload.len() as u32).to_be_bytes();

    send.write_all(&len)
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to write length: {}", e)))?;
    send.write_all(&payload)
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to write payload: {}", e)))?;

    Ok(())
}
