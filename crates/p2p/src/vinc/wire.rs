//! Message framing on vinc streams: a four-byte big-endian length, then the
//! CBOR body; a CAR reply is raw bytes up to the stream's end.

use std::time::Duration;

use vinc::{Hosted, RecvStream, SendStream};

use crate::error::{Error, Result};

/// A vinc error, with what was being done.
pub fn vinc_error(context: &str, error: vinc::Error) -> Error {
    match error.kind() {
        vinc::ErrorKind::Closed { .. } | vinc::ErrorKind::Shutdown => {
            Error::Transport(format!("{context}: connection closed: {error}"))
        }
        vinc::ErrorKind::Refused => Error::Unauthorized(format!("{context}: {error}")),
        vinc::ErrorKind::TimedOut => Error::Transport(format!("{context}: timed out: {error}")),
        _ => Error::Transport(format!("{context}: {error}")),
    }
}

pub fn encode<T>(value: &T) -> Result<Vec<u8>>
where
    T: serde::Serialize,
{
    defra_core::cbor::to_vec(value).map_err(|e| Error::Codec(e.to_string()))
}

/// Decode a CBOR body. Taking it by value frees it once decoded.
pub fn decode<T, Body>(payload: Body) -> Result<T>
where
    T: serde::de::DeserializeOwned,
    Body: AsRef<[u8]>,
{
    defra_core::cbor::from_slice(payload.as_ref()).map_err(|e| Error::Codec(e.to_string()))
}

/// Write an encoded body with its length prefix.
pub async fn write_payload<H>(send: &mut SendStream<H>, payload: &[u8]) -> Result<()>
where
    H: Hosted,
{
    let len = u32::try_from(payload.len())
        .map_err(|_| Error::Codec(format!("a {} byte message is too large", payload.len())))?;
    send.write_all(&len.to_be_bytes())
        .await
        .map_err(|e| vinc_error("write length", e))?;
    send.write_all(payload)
        .await
        .map_err(|e| vinc_error("write payload", e))
}

pub async fn read_length<H>(recv: &mut RecvStream<H>) -> Result<usize>
where
    H: Hosted,
{
    let mut len = [0u8; 4];
    recv.read_exact(&mut len)
        .await
        .map_err(|e| vinc_error("read length", e))?;
    Ok(u32::from_be_bytes(len) as usize)
}

/// Read a `len`-byte body, giving up once nothing arrives for `idle`.
/// `Ok(None)` means the sender stalled.
pub async fn read_body_until_idle<H>(
    recv: &mut RecvStream<H>,
    len: usize,
    idle: Duration,
) -> Result<Option<Vec<u8>>>
where
    H: Hosted,
{
    let mut payload = vec![0u8; len];
    let mut filled = 0;
    while filled < len {
        let Ok(read) = crate::rt::time::timeout(idle, recv.read(&mut payload[filled..])).await
        else {
            return Ok(None);
        };
        match read.map_err(|e| vinc_error("read payload", e))? {
            Some(n) => filled += n,
            None => {
                return Err(Error::Codec(format!(
                    "stream ended after {filled} of {len} payload bytes"
                )))
            }
        }
    }
    Ok(Some(payload))
}

/// Read a stream to its end, at most `limit` bytes, calling `on_chunk` with
/// each chunk's size.
pub async fn read_to_end_observed<H, OnChunk>(
    recv: &mut RecvStream<H>,
    limit: usize,
    mut on_chunk: OnChunk,
) -> Result<Vec<u8>>
where
    H: Hosted,
    OnChunk: FnMut(usize),
{
    let mut out = Vec::new();
    let mut buf = vec![0u8; vinc::stream::BLOCK];
    while let Some(n) = recv
        .read(&mut buf)
        .await
        .map_err(|e| vinc_error("read CAR", e))?
    {
        on_chunk(n);
        if out.len() + n > limit {
            return Err(Error::Transport(format!(
                "stream longer than {limit} bytes"
            )));
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}
