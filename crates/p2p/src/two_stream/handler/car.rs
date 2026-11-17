//! CAR stream protocol handler methods.

use cid::Cid;
use futures::AsyncReadExt;
use libp2p::PeerId;
use libp2p::Stream;

use crate::error::{Error, Result};
use crate::message::HeldCids;
use crate::sync::car::{MAX_HAVE_CIDS, MAX_PRESENT_CIDS};
use crate::two_stream::event::TwoStreamEvent;

use super::TwoStreamHandler;

async fn read_stream(
    stream: &mut Stream,
    max_size: u64,
    timeout: std::time::Duration,
) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    crate::rt::time::timeout(timeout, stream.take(max_size).read_to_end(&mut buf))
        .await
        .map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "CAR stream read timed out")
        })??;
    Ok(buf)
}

/// Separates the haves from the present CIDs. A CID starts with its version
/// (1) or, for CIDv0, its multihash code (0x12), never with 0.
const PRESENT_MARKER: u8 = 0;

/// A CAR request: the root CID's bytes, then each have CID's bytes, then,
/// when any are named, [`PRESENT_MARKER`] and each present CID's bytes. A CID
/// is self-delimiting, and a responder that predates haves reads the root
/// alone, so the request it sees is unchanged.
pub(crate) fn encode_car_request(root_cid: &Cid, held: &HeldCids) -> Vec<u8> {
    let mut out = root_cid.to_bytes();
    for have in held.haves.iter().take(MAX_HAVE_CIDS) {
        out.extend_from_slice(&have.to_bytes());
    }
    if !held.present.is_empty() {
        out.push(PRESENT_MARKER);
        for present in held.present.iter().take(MAX_PRESENT_CIDS) {
            out.extend_from_slice(&present.to_bytes());
        }
    }
    out
}

/// Reads whole CIDs from `cursor` up to `limit` or [`PRESENT_MARKER`],
/// stopping at the first torn one.
fn read_cids(cursor: &mut std::io::Cursor<&[u8]>, limit: usize) -> Vec<Cid> {
    let mut cids = Vec::new();
    while cids.len() < limit {
        let rest = &cursor.get_ref()[cursor.position() as usize..];
        if rest.first().is_none_or(|byte| *byte == PRESENT_MARKER) {
            break;
        }
        match Cid::read_bytes(&mut *cursor) {
            Ok(cid) => cids.push(cid),
            Err(_) => break,
        }
    }
    cids
}

/// The root and what a CAR request names as held: at most [`MAX_HAVE_CIDS`]
/// haves and [`MAX_PRESENT_CIDS`] present CIDs, ignoring any bytes after the
/// last whole CID.
pub(crate) fn decode_car_request(buf: &[u8]) -> Result<(Cid, HeldCids)> {
    let mut cursor = std::io::Cursor::new(buf);
    let root_cid = Cid::read_bytes(&mut cursor)
        .map_err(|e| Error::InvalidCid(format!("invalid CID in CAR request: {}", e)))?;
    let haves = read_cids(&mut cursor, MAX_HAVE_CIDS);
    let mut present = Vec::new();
    if buf.get(cursor.position() as usize) == Some(&PRESENT_MARKER) {
        cursor.set_position(cursor.position() + 1);
        present = read_cids(&mut cursor, MAX_PRESENT_CIDS);
    }
    Ok((root_cid, HeldCids { haves, present }))
}

#[cfg(test)]
#[path = "../../../tests/unit/car_request_stream.rs"]
mod request_tests;

impl TwoStreamHandler {
    /// Handle an incoming CAR request stream.
    ///
    /// The request is raw CID bytes (self-describing, no CBOR wrapper).
    pub async fn handle_car_request_stream(
        peer_id: PeerId,
        mut stream: Stream,
        max_car_size: u64,
        stream_read_timeout: std::time::Duration,
    ) -> Result<TwoStreamEvent> {
        let buf = read_stream(&mut stream, max_car_size, stream_read_timeout)
            .await
            .map_err(|e| Error::Transport(format!("failed to read CAR request: {}", e)))?;

        let (root_cid, held) = decode_car_request(&buf)?;

        Ok(TwoStreamEvent::CarFetchRequest {
            peer_id,
            root_cid,
            held,
        })
    }

    /// Handle an incoming CAR response stream.
    ///
    /// The response is raw CARv1 bytes. The first root CID from the header
    /// is extracted to identify which request this responds to.
    pub async fn handle_car_response_stream(
        peer_id: PeerId,
        mut stream: Stream,
        max_car_size: u64,
        stream_read_timeout: std::time::Duration,
    ) -> Result<TwoStreamEvent> {
        let car_data = read_stream(&mut stream, max_car_size, stream_read_timeout)
            .await
            .map_err(|e| Error::Transport(format!("failed to read CAR response: {}", e)))?;

        // Extract the root CID from the CAR header for event routing
        let (roots, _) = crate::sync::car::decode_car(&car_data)?;
        let root_cid = roots
            .into_iter()
            .next()
            .ok_or_else(|| Error::Codec("CAR response has no roots".into()))?;

        Ok(TwoStreamEvent::CarFetchResponse {
            peer_id,
            root_cid,
            car_data,
        })
    }

    /// Send a CAR request to a peer (raw CID bytes on the CAR request protocol).
    pub async fn send_car_request(
        &mut self,
        peer_id: PeerId,
        root_cid: Cid,
        held: HeldCids,
    ) -> Result<()> {
        use futures::AsyncWriteExt;

        let mut stream = self
            .control
            .open_stream(peer_id, Self::car_request_protocol())
            .await
            .map_err(|e| Error::Transport(format!("failed to open CAR request stream: {}", e)))?;

        stream
            .write_all(&encode_car_request(&root_cid, &held))
            .await
            .map_err(|e| Error::Transport(format!("failed to write CAR request: {}", e)))?;
        stream
            .close()
            .await
            .map_err(|e| Error::Transport(format!("failed to close CAR request stream: {}", e)))?;

        tracing::debug!(
            peer_id = %peer_id,
            root_cid = %root_cid,
            "Sent CAR request"
        );

        Ok(())
    }

    /// Send a CAR response to a peer (raw CARv1 bytes on the CAR response protocol).
    pub async fn send_car_response(&mut self, peer_id: PeerId, car_data: Vec<u8>) -> Result<()> {
        use futures::AsyncWriteExt;

        let mut stream = self
            .control
            .open_stream(peer_id, Self::car_response_protocol())
            .await
            .map_err(|e| Error::Transport(format!("failed to open CAR response stream: {}", e)))?;

        stream
            .write_all(&car_data)
            .await
            .map_err(|e| Error::Transport(format!("failed to write CAR response: {}", e)))?;
        stream
            .close()
            .await
            .map_err(|e| Error::Transport(format!("failed to close CAR response stream: {}", e)))?;

        tracing::debug!(
            peer_id = %peer_id,
            car_bytes = car_data.len(),
            "Sent CAR response"
        );

        Ok(())
    }
}
