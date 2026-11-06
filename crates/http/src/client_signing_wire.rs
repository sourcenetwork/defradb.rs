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
mod tests {
    use super::*;
    use futures::{channel::mpsc, StreamExt};
    use serde_json::json;

    #[test]
    fn large_signing_message_uses_bounded_rfc6455_fragments() {
        edge::testing::edge()
            .block_on(async {
                let (mut sender, receiver) = mpsc::unbounded::<Message>();
                let value = json!({"input": "session history ✓".repeat(120_000)});
                let expected = serde_json::to_vec(&value).unwrap();
                assert!(expected.len() > MAX_FRAME);
                assert!(send(&mut sender, &value).await.is_ok());
                drop(sender);
                let frames: Vec<_> = receiver.collect().await;
                assert!(frames.len() > 1);
                let mut actual = Vec::new();
                let count = frames.len();
                for (index, message) in frames.into_iter().enumerate() {
                    let Message::Frame(frame) = message else {
                        panic!("expected fragmented frame")
                    };
                    assert_eq!(
                        frame.header().opcode,
                        OpCode::Data(if index == 0 {
                            Data::Text
                        } else {
                            Data::Continue
                        })
                    );
                    assert_eq!(frame.header().is_final, index + 1 == count);
                    let data = frame.into_payload();
                    assert!(data.len() <= CHUNK_SIZE);
                    actual.extend_from_slice(&data);
                }
                assert_eq!(actual, expected);
            })
            .unwrap();
    }

    #[test]
    fn oversized_message_is_rejected_before_any_frame_is_sent() {
        edge::testing::edge()
            .block_on(async {
                let (mut sender, receiver) = mpsc::unbounded::<Message>();
                let value = json!({"input": "x".repeat(MAX_MESSAGE)});
                assert!(matches!(
                    send(&mut sender, &value).await,
                    Err(WireError::TooLarge)
                ));
                drop(sender);
                assert!(receiver.collect::<Vec<_>>().await.is_empty());
            })
            .unwrap();
    }
}
