//! The mux ALPN, its stream tags, and wire format helpers for iroh transport.
//!
//! Every Defra protocol shares one QUIC connection per peer. The connection is
//! negotiated on [`ALPN_MUX`]; the protocol a stream carries is named by a tag
//! written as that stream's first frame. Putting the discriminator in-band
//! rather than in the ALPN is what lets one connection serve all of them — ALPN
//! is negotiated once per TLS handshake, so an ALPN-per-protocol design costs a
//! connection, a congestion controller, and a hole-punch per protocol.

use iroh::endpoint::{RecvStream, SendStream};

/// The single ALPN every Defra protocol is multiplexed over.
pub const ALPN_MUX: &[u8] = b"/defra-iroh/mux/0.1";

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

/// Stream tag for the two-stream push protocol.
pub const STREAM_TWOSTREAM: &[u8] = b"/defra-iroh/twostream/0.1";

/// Stream tag for two-stream push replies.
pub const STREAM_TWOSTREAM_RESP: &[u8] = b"/defra-iroh/twostream/0.1/resp";

/// Every stream tag this node dispatches, for the invariants asserted below.
///
/// Dispatch matches tags individually, so this list exists to prove the set is
/// distinct, framable, and disjoint from the ALPN it travels on.
#[cfg(test)]
pub const ALL_STREAM_TAGS: &[&[u8]] = &[
    STREAM_PUSHLOG,
    STREAM_DOCSYNC,
    STREAM_DOCSYNC_RESP,
    STREAM_BRANCHABLE,
    STREAM_BRANCHABLE_RESP,
    STREAM_CAR,
    STREAM_CAR_RESP,
    STREAM_IDENTITY,
    STREAM_SE,
    STREAM_SE_QUERY_REQ,
    STREAM_SE_QUERY_RESP,
    STREAM_TWOSTREAM,
    STREAM_TWOSTREAM_RESP,
    STREAM_MANAGE_REQ,
    STREAM_MANAGE_RESP,
    STREAM_MANAGE_QUERY_REQ,
    STREAM_MANAGE_QUERY_RESP,
];

/// Upper bound on a stream tag, enforced on read so a peer cannot make us
/// allocate on a length it never intends to fill.
pub const MAX_STREAM_TAG_LEN: usize = 64;

/// Maximum size for general messages (matches libp2p default).
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

/// Maximum size for CAR block transfers (matches libp2p default).
pub const MAX_CAR_SIZE: usize = 64 * 1024 * 1024; // 64 MiB

/// Maximum size for management messages.
pub const MAX_MANAGE_MSG_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

/// Write the tag naming the protocol this stream carries.
///
/// Every stream opened on [`ALPN_MUX`] begins with this frame, before any
/// protocol payload. The one-byte length distinguishes it from the four-byte
/// message length prefix that follows.
pub async fn write_stream_tag(send: &mut SendStream, tag: &[u8]) -> crate::error::Result<()> {
    if tag.is_empty() || tag.len() > MAX_STREAM_TAG_LEN {
        return Err(crate::error::Error::Codec(format!(
            "stream tag of {} bytes does not fit a tag frame",
            tag.len()
        )));
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
    let payload = read_message_bytes(recv, max_size).await?;
    defra_core::cbor::from_slice(&payload).map_err(|e| crate::error::Error::Codec(e.to_string()))
}

/// Read only the length-prefixed payload bytes from a QUIC recv stream.
pub async fn read_message_bytes(
    recv: &mut RecvStream,
    max_size: usize,
) -> crate::error::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to read length: {}", e)))?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > max_size {
        return Err(crate::error::Error::Codec(format!(
            "message too large: {} bytes exceeds limit of {} bytes",
            len, max_size
        )));
    }

    let mut payload = vec![0u8; len];
    recv.read_exact(&mut payload)
        .await
        .map_err(|e| crate::error::Error::Codec(format!("failed to read payload: {}", e)))?;
    Ok(payload)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manage_tags_registered() {
        for tag in [
            STREAM_MANAGE_REQ,
            STREAM_MANAGE_RESP,
            STREAM_MANAGE_QUERY_REQ,
            STREAM_MANAGE_QUERY_RESP,
        ] {
            assert!(ALL_STREAM_TAGS.contains(&tag));
        }
    }

    #[test]
    fn every_tag_fits_a_stream_tag_frame() {
        for tag in ALL_STREAM_TAGS {
            assert!(
                !tag.is_empty() && tag.len() <= MAX_STREAM_TAG_LEN,
                "tag {:?} does not fit a stream tag frame",
                String::from_utf8_lossy(tag)
            );
        }
    }

    #[test]
    fn tags_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for tag in ALL_STREAM_TAGS {
            assert!(
                seen.insert(*tag),
                "duplicate stream tag {:?}",
                String::from_utf8_lossy(tag)
            );
        }
    }

    #[test]
    fn no_tag_collides_with_the_mux_alpn() {
        assert!(!ALL_STREAM_TAGS.contains(&ALPN_MUX));
    }
}
