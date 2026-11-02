mod common;

use asynchronous_codec::{Decoder, Encoder, Framed};
use bitswap::{BitswapCodec, BitswapHandlerError, BitswapMessage, ProtocolId, WantType};
use bytes::{Bytes, BytesMut};
use common::{block_v1, cid_v1};
use futures::{SinkExt, StreamExt};
use unsigned_varint::codec::UviBytes;

const MAX: usize = 2 * 1024 * 1024;

fn codec(protocol: ProtocolId) -> BitswapCodec {
    let mut l = UviBytes::default();
    l.set_max_len(MAX);
    BitswapCodec::new(l, protocol)
}

fn sample() -> BitswapMessage {
    let mut m = BitswapMessage::new(true);
    m.add_entry(cid_v1(b"w"), 2, WantType::Have, true);
    m.add_block(block_v1(b"data"));
    m.add_have(cid_v1(b"h"));
    m.set_pending_bytes(3);
    m
}

#[tokio::test]
async fn framed_roundtrip_v1() {
    let (a, b) = tokio::io::duplex(64 * 1024);
    let mut tx = Framed::new(
        tokio_util::compat::TokioAsyncReadCompatExt::compat(a),
        codec(ProtocolId::Bitswap120),
    );
    let mut rx = Framed::new(
        tokio_util::compat::TokioAsyncReadCompatExt::compat(b),
        codec(ProtocolId::Bitswap120),
    );
    let m = sample();
    tx.send(m.clone()).await.unwrap();
    let (got, proto) = rx.next().await.unwrap().unwrap();
    assert_eq!(got, m);
    assert_eq!(proto, ProtocolId::Bitswap120);
}

#[test]
fn v0_protocols_drop_presences_and_pending() {
    for p in [ProtocolId::Legacy, ProtocolId::Bitswap100] {
        let mut c = codec(p);
        let mut dst = BytesMut::new();
        c.encode(sample(), &mut dst).unwrap();
        let (got, _) = c.decode(&mut dst).unwrap().unwrap();
        assert_eq!(got.block_presences().count(), 0);
        assert_eq!(got.pending_bytes(), 0);
        assert_eq!(got.blocks_len(), 1);
        assert!(got.full());
    }
}

#[test]
fn partial_frame_waits_for_more_bytes() {
    let mut c = codec(ProtocolId::Bitswap110);
    let mut dst = BytesMut::new();
    c.encode(sample(), &mut dst).unwrap();
    let mut head = dst.split_to(dst.len() - 1);
    assert!(c.decode(&mut head).unwrap().is_none());
    head.extend_from_slice(&dst);
    assert!(c.decode(&mut head).unwrap().is_some());
}

fn v0_message_with_payload_len(total: usize) -> BitswapMessage {
    // empty wantlist (2 bytes) + tag (1) + 3-byte length varint
    let data = vec![7u8; total - 6];
    let mut m = BitswapMessage::new(false);
    m.add_block(bitswap::Block::from_v0_data(Bytes::from(data)).unwrap());
    m
}

#[test]
fn exactly_max_is_accepted_and_one_more_rejected_on_encode() {
    let mut c = codec(ProtocolId::Bitswap100);
    let mut dst = BytesMut::new();
    c.encode(v0_message_with_payload_len(MAX), &mut dst)
        .unwrap();
    let (got, _) = c.decode(&mut dst).unwrap().unwrap();
    assert_eq!(got.blocks_len(), 1);

    let mut dst = BytesMut::new();
    assert!(matches!(
        c.encode(v0_message_with_payload_len(MAX + 1), &mut dst),
        Err(BitswapHandlerError::MaxTransmissionSize)
    ));
}

#[test]
fn one_over_max_is_rejected_on_decode() {
    let mut c = codec(ProtocolId::Bitswap100);
    let mut frame = BytesMut::new();
    let mut buf = unsigned_varint::encode::usize_buffer();
    frame.extend_from_slice(unsigned_varint::encode::usize(MAX + 1, &mut buf));
    frame.extend_from_slice(&vec![0u8; MAX + 1]);
    assert!(matches!(
        c.decode(&mut frame),
        Err(BitswapHandlerError::MaxTransmissionSize)
    ));
}

fn oversized_frame() -> BytesMut {
    let mut frame = BytesMut::new();
    let mut buf = unsigned_varint::encode::usize_buffer();
    frame.extend_from_slice(unsigned_varint::encode::usize(MAX + 1, &mut buf));
    frame.extend_from_slice(&vec![0u8; MAX + 1]);
    frame
}

/// Decodes every frame `stream` holds when it arrives in pieces of `chunk`
/// bytes, counting rejected oversized frames.
fn decode_in_chunks(
    c: &mut BitswapCodec,
    stream: &[u8],
    chunk: usize,
) -> (Vec<BitswapMessage>, usize) {
    let (mut messages, mut rejected) = (Vec::new(), 0);
    let mut src = BytesMut::new();
    for piece in stream.chunks(chunk) {
        src.extend_from_slice(piece);
        loop {
            match c.decode(&mut src) {
                Ok(Some((message, _))) => messages.push(message),
                Ok(None) => break,
                Err(BitswapHandlerError::MaxTransmissionSize) => rejected += 1,
                Err(error) => panic!("{error}"),
            }
        }
    }
    (messages, rejected)
}

/// #2414: the body of a rejected frame is skipped, not read as frames, so
/// the valid frame after it is the only message, however the bytes arrive.
#[test]
fn the_frame_after_an_oversized_one_decodes_alone() {
    let mut valid = BytesMut::new();
    codec(ProtocolId::Bitswap120)
        .encode(sample(), &mut valid)
        .unwrap();
    let mut stream = oversized_frame();
    stream.extend_from_slice(&valid);
    for chunk in [stream.len(), 64 * 1024, 4093, 1] {
        let mut c = codec(ProtocolId::Bitswap120);
        let (messages, rejected) = decode_in_chunks(&mut c, &stream, chunk);
        assert_eq!(rejected, 1, "chunk {chunk}");
        assert_eq!(messages, vec![sample()], "chunk {chunk}");
    }
}

/// A frame whose body arrives after its prefix is read whole, even when the
/// body's first bytes would read as an oversized length prefix.
#[test]
fn a_split_body_is_never_read_as_a_prefix() {
    // 0xff bytes read as a varint far over the maximum.
    let mut message = BitswapMessage::new(false);
    message.add_block(bitswap::Block::from_v0_data(Bytes::from(vec![0xffu8; 4096])).unwrap());
    let mut stream = BytesMut::new();
    codec(ProtocolId::Bitswap100)
        .encode(message.clone(), &mut stream)
        .unwrap();
    for chunk in [100, 333, 1000] {
        let mut c = codec(ProtocolId::Bitswap100);
        let (messages, rejected) = decode_in_chunks(&mut c, &stream, chunk);
        assert_eq!(rejected, 0, "chunk {chunk}");
        assert_eq!(messages, vec![message.clone()], "chunk {chunk}");
    }
}

#[test]
fn bad_message_inside_frame_is_a_bitswap_error() {
    let mut c = codec(ProtocolId::Bitswap120);
    let mut frame = BytesMut::from(&[3u8, 0x0a, 0x05, 0x01][..]);
    assert!(matches!(
        c.decode(&mut frame),
        Err(BitswapHandlerError::Bitswap(_))
    ));
}
