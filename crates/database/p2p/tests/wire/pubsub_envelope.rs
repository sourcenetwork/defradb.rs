//! The pubsub-rpc response envelope (`InternalResponse`) the KMS key exchange
//! still rides on, pinned to the bytes Go's `go-libp2p-pubsub-rpc` writes.

use p2p::message::pubsub::{DocSyncReply, DocSyncRequest};
use p2p::pubsub_rpc::{derive_request_id, InternalResponse};

// Fixture bytes produced by `testdata/gen_pubsub_rpc_fixture/main.go`,
// which runs the same `ipld.Marshal(dagcbor.Encode, ...)` pipeline as
// `sourcenetwork/go-libp2p-pubsub-rpc` (see rpc.go:389).
//
// To regenerate after changing the fixture values:
//   cd testdata/gen_pubsub_rpc_fixture && go run main.go
const GO_FIXTURE_OK_HEX: &str = "a4624944783b6261666b7265696864776463656667683464716b6a763637757a636d77376f6a6565367865647a6465746f6a757a6a657674656e78717576796b75634572726064446174614568656c6c6f6446726f6d582212200102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
const GO_FIXTURE_ERR_HEX: &str = "a4624944783b6261666b7265696864776463656667683464716b6a763637757a636d77376f6a6565367865647a6465746f6a757a6a657674656e78717576796b75634572726b756e6b6e6f776e20646f636444617461406446726f6d40";

// The generator's end-to-end replay fixture: a DocSync request for `docA`,
// and the envelope Go answers it with, holding a reply from
// "12D3KooWRustPeer" whose head is CIDv1(raw, sha256("docA-head")).
const GO_REPLAY_REQUEST_HEX: &str = "a166646f634944738164646f6341";
const GO_REPLAY_ENVELOPE_HEX: &str = "a4624944783b6261666b726569626579647533777a736d73677a687473747967366169623667377565667575637075786e7570726e75377366796d6c366a68776d63457272606444617461585ba267726573756c747381a265646f63494464646f634165686561647381582401551220ac695a770969c9a3ae934d1e3325839c60bfe0e0d265e5494748ea46939d19a66673656e64657270313244334b6f6f5752757374506565726446726f6d40";

fn fixture_ok() -> InternalResponse {
    InternalResponse {
            id: "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku".to_string(),
            err: String::new(),
            data: b"hello".to_vec(),
            from: b"\x12\x20\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f\x20".to_vec(),
        }
}

fn fixture_err() -> InternalResponse {
    InternalResponse {
        id: "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku".to_string(),
        err: "unknown doc".to_string(),
        data: Vec::new(),
        from: Vec::new(),
    }
}

#[test]
fn encodes_byte_identical_to_go_ok_fixture() {
    let got = fixture_ok().to_cbor().expect("encode");
    assert_eq!(
        hex::encode(&got),
        GO_FIXTURE_OK_HEX,
        "ok fixture bytes must match Go's dag-cbor output exactly"
    );
}

#[test]
fn encodes_byte_identical_to_go_err_fixture() {
    let got = fixture_err().to_cbor().expect("encode");
    assert_eq!(
        hex::encode(&got),
        GO_FIXTURE_ERR_HEX,
        "err fixture bytes must match Go's dag-cbor output exactly"
    );
}

#[test]
fn decodes_go_ok_fixture() {
    let bytes = hex::decode(GO_FIXTURE_OK_HEX).expect("hex");
    let decoded = InternalResponse::from_cbor(&bytes).expect("decode");
    assert_eq!(decoded, fixture_ok());
}

#[test]
fn decodes_go_err_fixture() {
    let bytes = hex::decode(GO_FIXTURE_ERR_HEX).expect("hex");
    let decoded = InternalResponse::from_cbor(&bytes).expect("decode");
    assert_eq!(decoded, fixture_err());
}

#[test]
fn round_trip() {
    for r in [fixture_ok(), fixture_err()] {
        let bytes = r.to_cbor().expect("encode");
        let decoded = InternalResponse::from_cbor(&bytes).expect("decode");
        assert_eq!(decoded, r);
    }
}

#[test]
fn encodes_as_definite_length_map() {
    let bytes = fixture_ok().to_cbor().expect("encode");
    // A 4-field definite-length map is encoded as 0xa4 (major type 5, length 4).
    assert_eq!(bytes.first(), Some(&0xa4));
}

/// Go's replay envelope decodes here, its id is the request's id, and the
/// reply it holds re-encodes to the same bytes.
#[test]
fn go_replay_envelope_matches_its_request_and_reply() {
    let mut request = Vec::new();
    ciborium::into_writer(&DocSyncRequest::new(vec!["docA".to_string()]), &mut request)
        .expect("encode request");
    assert_eq!(hex::encode(&request), GO_REPLAY_REQUEST_HEX);

    let bytes = hex::decode(GO_REPLAY_ENVELOPE_HEX).expect("hex");
    let envelope = InternalResponse::from_cbor(&bytes).expect("envelope decode");
    assert_eq!(envelope.id, derive_request_id(&request).to_string());
    assert!(envelope.err.is_empty());
    assert!(envelope.from.is_empty());

    let reply: DocSyncReply =
        ciborium::from_reader(envelope.data.as_slice()).expect("inner reply decode");
    assert_eq!(reply.sender, "12D3KooWRustPeer");
    assert_eq!(reply.results.len(), 1);
    assert_eq!(reply.results[0].doc_id, "docA");
    let head = cid::Cid::try_from(reply.results[0].heads[0].as_slice()).expect("head is a CID");
    assert_eq!(head.codec(), 0x55, "head is CIDv1 raw");

    let mut reencoded = Vec::new();
    ciborium::into_writer(&reply, &mut reencoded).expect("encode reply");
    assert_eq!(reencoded, envelope.data);
    assert_eq!(
        hex::encode(envelope.to_cbor().expect("encode envelope")),
        GO_REPLAY_ENVELOPE_HEX
    );
}
