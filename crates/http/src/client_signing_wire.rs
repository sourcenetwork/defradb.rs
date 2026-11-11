use async_tungstenite::tungstenite::{
    protocol::frame::{
        coding::{Data, OpCode},
        Frame,
    },
    Message,
};
use futures::{Sink, SinkExt, Stream, StreamExt};
use serde_json::Value;
use std::io::{self, Write};

pub(crate) const MAX_FRAME: usize = 1024 * 1024;
/// Bounds each encoded logical message, including hex-encoded signing blocks.
/// JSON and hex conversion run on the socket task; this limit does not create
/// a separate executor for that work.
pub(crate) const MAX_MESSAGE: usize = 16 * 1024 * 1024;
const CHUNK_SIZE: usize = 256 * 1024;

pub(crate) enum WireError {
    TooLarge,
    Connection(String),
    Protocol(String),
}

#[derive(Default)]
struct BoundedBuffer {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Write for BoundedBuffer {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.len() > MAX_MESSAGE - self.bytes.len() {
            self.exceeded = true;
            return Err(io::Error::other("signing message too large"));
        }
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) async fn send<S, E>(socket: &mut S, value: &Value) -> Result<(), WireError>
where
    S: Sink<Message, Error = E> + Unpin,
    E: std::fmt::Display,
{
    let mut buffer = BoundedBuffer::default();
    if let Err(error) = serde_json::to_writer(&mut buffer, value) {
        return Err(if buffer.exceeded {
            WireError::TooLarge
        } else {
            WireError::Protocol(error.to_string())
        });
    }
    let data = buffer.bytes;
    if data.len() <= CHUNK_SIZE {
        let text =
            String::from_utf8(data).map_err(|error| WireError::Protocol(error.to_string()))?;
        return socket
            .send(Message::Text(text.into()))
            .await
            .map_err(|error| WireError::Connection(error.to_string()));
    }
    let chunks = data.chunks(CHUNK_SIZE);
    let count = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        let opcode = if index == 0 {
            Data::Text
        } else {
            Data::Continue
        };
        let frame = Frame::message(chunk.to_vec(), OpCode::Data(opcode), index + 1 == count);
        socket
            .send(Message::Frame(frame))
            .await
            .map_err(|error| WireError::Connection(error.to_string()))?;
    }
    Ok(())
}

pub(crate) async fn receive<S, E>(socket: &mut S) -> Result<Value, WireError>
where
    S: Stream<Item = Result<Message, E>> + Unpin,
    E: std::fmt::Display,
{
    let message = socket
        .next()
        .await
        .ok_or_else(|| WireError::Connection("connection closed before a message".into()))?
        .map_err(|error| WireError::Connection(error.to_string()))?;
    let Message::Text(text) = message else {
        return Err(WireError::Protocol("expected a text message".into()));
    };
    if text.len() > MAX_MESSAGE {
        return Err(WireError::TooLarge);
    }
    serde_json::from_str(&text).map_err(|error| WireError::Protocol(error.to_string()))
}

#[cfg(test)]
#[path = "../tests/unit/client_signing_wire.rs"]
mod tests;
