//! CAR stream protocol handler methods.

use cid::Cid;
use futures::AsyncReadExt;
use libp2p::PeerId;
use libp2p::Stream;

use crate::error::{Error, Result};
use crate::sync::car::MAX_HAVE_CIDS;
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

/// A CAR request: the root CID's bytes, then each have CID's bytes. A CID is
/// self-delimiting, and a responder that predates haves reads the root alone,
/// so the request it sees is unchanged.
pub(crate) fn encode_car_request(root_cid: &Cid, haves: &[Cid]) -> Vec<u8> {
    let mut out = root_cid.to_bytes();
    for have in haves.iter().take(MAX_HAVE_CIDS) {
        out.extend_from_slice(&have.to_bytes());
    }
    out
}

/// The root and the haves of a CAR request: at most [`MAX_HAVE_CIDS`] haves,
/// ignoring any bytes after the last whole CID.
pub(crate) fn decode_car_request(buf: &[u8]) -> Result<(Cid, Vec<Cid>)> {
    let mut cursor = std::io::Cursor::new(buf);
    let root_cid = Cid::read_bytes(&mut cursor)
        .map_err(|e| Error::InvalidCid(format!("invalid CID in CAR request: {}", e)))?;
    let mut haves = Vec::new();
    while (cursor.position() as usize) < buf.len() && haves.len() < MAX_HAVE_CIDS {
        match Cid::read_bytes(&mut cursor) {
            Ok(have) => haves.push(have),
            Err(_) => break,
        }
    }
    Ok((root_cid, haves))
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

        let (root_cid, haves) = decode_car_request(&buf)?;

        Ok(TwoStreamEvent::CarFetchRequest {
            peer_id,
            root_cid,
            haves,
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
        haves: Vec<Cid>,
    ) -> Result<()> {
        use futures::AsyncWriteExt;

        let mut stream = self
            .control
            .open_stream(peer_id, Self::car_request_protocol())
            .await
            .map_err(|e| Error::Transport(format!("failed to open CAR request stream: {}", e)))?;

        stream
            .write_all(&encode_car_request(&root_cid, &haves))
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
