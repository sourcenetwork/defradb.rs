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
