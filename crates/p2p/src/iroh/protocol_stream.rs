//! The two halves of an embedder protocol's stream.
//!
//! Wrapped rather than handed out as iroh's own types, for two reasons. A
//! dropped iroh send half finishes its stream, so a reply cut short by a
//! deadline would read as complete; these reset instead. And an embedder's
//! code does not change when the node moves its iroh pin.

use iroh::endpoint::{ReadError, RecvStream, SendStream, WriteError};

use crate::error::{Error, Result};

/// QUIC code a protocol stream is reset with when its half is dropped before
/// it was finished: the handler gave up, or the node cancelled it at its
/// deadline, on a closed connection or at shutdown.
pub const RESET_CANCELLED: u32 = 7;

/// The send half of a protocol stream. Dropping it unfinished resets the
/// stream with [`RESET_CANCELLED`], so the other side never mistakes a cut
/// short reply for a whole one.
pub struct ProtocolSendStream(Option<SendStream>);

impl ProtocolSendStream {
    pub(super) fn new(inner: SendStream) -> Self {
        Self(Some(inner))
    }

    /// Writes all of `bytes`.
    pub async fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        let Some(send) = self.0.as_mut() else {
            return Err(Error::ConnectionClosed);
        };
        send.write_all(bytes).await.map_err(write_error)
    }

    /// Ends the stream after what was written.
    pub fn finish(mut self) -> Result<()> {
        let Some(mut send) = self.0.take() else {
            return Err(Error::ConnectionClosed);
        };
        send.finish()
            .map_err(|error| Error::Transport(error.to_string()))
    }
}

impl Drop for ProtocolSendStream {
    fn drop(&mut self) {
        if let Some(mut send) = self.0.take() {
            let _ = send.reset(RESET_CANCELLED.into());
        }
    }
}

/// The receive half of a protocol stream. Dropping it stops the stream.
pub struct ProtocolRecvStream(RecvStream);

impl ProtocolRecvStream {
    pub(super) fn new(inner: RecvStream) -> Self {
        Self(inner)
    }

    /// Reads into `buf`: how many bytes, or `None` once the stream ended.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>> {
        self.0.read(buf).await.map_err(read_error)
    }

    /// Reads to the end of the stream, refusing more than `limit` bytes.
    pub async fn read_to_end(&mut self, limit: usize) -> Result<Vec<u8>> {
        self.0
            .read_to_end(limit)
            .await
            .map_err(|error| match error {
                iroh::endpoint::ReadToEndError::Read(error) => read_error(error),
                iroh::endpoint::ReadToEndError::TooLong => {
                    Error::Codec(format!("stream is longer than {limit} bytes"))
                }
            })
    }
}

fn write_error(error: WriteError) -> Error {
    match error {
        WriteError::Stopped(code) => Error::StreamReset(code.into_inner()),
        WriteError::ConnectionLost(_) => Error::ConnectionClosed,
        other => Error::Transport(other.to_string()),
    }
}

fn read_error(error: ReadError) -> Error {
    match error {
        ReadError::Reset(code) => Error::StreamReset(code.into_inner()),
        ReadError::ConnectionLost(_) => Error::ConnectionClosed,
        other => Error::Transport(other.to_string()),
    }
}
