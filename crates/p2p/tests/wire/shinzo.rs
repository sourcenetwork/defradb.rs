use bytes::Bytes;
use p2p::message::car_exchange::{is_cbor_map, CarReply, CarRequest};
use p2p::message::{PushLogBroadcast, PushLogRequest, MAX_INBOUND_DOCUMENTS};

// Byte fixtures from `testdata/gen_message_fixtures/main.go`, which mirrors Go
// shinzo-develop's `protocol.CARRequest`, `CARReply`, `PushLogRequest` and
// `DocumentInfo` and encodes them with fxamacker/cbor's default options.
const GO_CAR_REQUEST_HEX: &str = "a66756657273696f6e6e2f646566726164622f302e302e31694d6573736167654944676d73672d6361726853656e646572494467706565722d676f665075626b6579420102695369676e6174757265420304644349447382430171aa430171bb";
const GO_CAR_REPLY_HEX: &str = "a66756657273696f6e6e2f646566726164622f302e302e31694d6573736167654944676d73672d6361726853656e646572494467706565722d676f665075626b6579420102695369676e6174757265420304644341527383420a0bf640";
const GO_CAR_REPLY_ERROR_HEX: &str = "a66756657273696f6e6e2f646566726164622f302e302e31694d6573736167654944676d73672d6361726853656e646572494467706565722d676f665075626b65794201026a4572724d65737361676564626f6f6d6443415273f6";
const GO_PUSH_LOG_REQUEST_CAR_HEX: &str = "ab6756657273696f6e6e2f646566726164622f302e302e31694d6573736167654944666d73672d676f6853656e646572494467706565722d676f665075626b6579420102695369676e617475726542030465446f63494468626166792d646f6363434944430171aa6c436f6c6c656374696f6e49446f626166792d636f6c6c656374696f6e6743726561746f726b6469643a6b65793a7a476f65426c6f636b44a161780163636172420a0b";
const GO_PUSH_LOG_BATCH_HEX: &str = "aa6756657273696f6e60694d6573736167654944606853656e646572494460665075626b6579f665446f6349446063434944f66c436f6c6c656374696f6e49446f626166792d636f6c6c656374696f6e6743726561746f726a313244334b6f6f57476f65426c6f636bf669646f63756d656e747382a365446f63494465646f632d6163434944430171aa65426c6f636b41a1a465446f63494465646f632d6263434944430171bb65426c6f636b41a263636172410c";

fn bytes_of(hex_text: &str) -> Vec<u8> {
    hex::decode(hex_text).expect("fixture hex")
}

fn encode<T>(value: &T) -> Vec<u8>
where
    T: serde::Serialize,
{
    defra_core::cbor::to_vec(value).expect("encode")
}

#[test]
fn car_request_reads_and_writes_go_bytes() {
    let fixture = bytes_of(GO_CAR_REQUEST_HEX);
    let request: CarRequest = defra_core::cbor::from_slice(&fixture).expect("decode");
    assert_eq!(request.message_id, "msg-car");
    assert_eq!(request.signature.as_deref(), Some(&[0x03, 0x04][..]));
    let cids: Vec<&[u8]> = request.cid_bytes().collect();
    assert_eq!(cids, [&[0x01, 0x71, 0xaa][..], &[0x01, 0x71, 0xbb][..]]);
    assert_eq!(encode(&request), fixture);
}

#[test]
fn car_reply_keeps_nil_and_empty_entries_apart() {
    let fixture = bytes_of(GO_CAR_REPLY_HEX);
    let reply: CarReply = defra_core::cbor::from_slice(&fixture).expect("decode");
    let cars: Vec<&[u8]> = reply.car_bytes().collect();
    assert_eq!(cars, [&[0x0a, 0x0b][..], &[][..], &[][..]]);
    assert_eq!(reply.len(), 3);
    // Re-encoding gives back Go's bytes, so a Go signature still checks out.
    assert_eq!(encode(&reply), fixture);
}

#[test]
fn car_reply_error_reads_and_writes_go_bytes() {
    let fixture = bytes_of(GO_CAR_REPLY_ERROR_HEX);
    let reply: CarReply = defra_core::cbor::from_slice(&fixture).expect("decode");
    assert_eq!(reply.err_message.as_deref(), Some("boom"));
    assert!(reply.cars.is_none());
    assert!(reply.is_empty());
    assert_eq!(encode(&reply), fixture);
}

#[test]
fn rust_reply_writes_an_unserved_head_as_go_nil() {
    let mut reply = CarReply::new("msg-car", vec![vec![0x0a, 0x0b], Vec::new()]);
    reply.sender_id = "peer-go".into();
    reply.pubkey = vec![0x01, 0x02];
    reply.signature = Some(vec![0x03, 0x04]);
    let mut go = bytes_of(GO_CAR_REPLY_HEX);
    // Go's fixture has a third, empty-but-not-nil entry: drop it and fix the
    // array length to compare the first two.
    go.truncate(go.len() - 1);
    let array_header = go.len() - 5;
    assert_eq!(go[array_header], 0x83);
    go[array_header] = 0x82;
    assert_eq!(encode(&reply), go);
}

#[test]
fn push_log_request_reads_and_writes_inline_car() {
    let fixture = bytes_of(GO_PUSH_LOG_REQUEST_CAR_HEX);
    let request: PushLogRequest = defra_core::cbor::from_slice(&fixture).expect("decode");
    assert_eq!(request.car.as_ref(), &[0x0a, 0x0b]);
    assert!(request.documents.is_empty());
    assert_eq!(encode(&request), fixture);
}

#[test]
fn go_batch_becomes_one_broadcast_per_document() {
    let fixture = bytes_of(GO_PUSH_LOG_BATCH_HEX);
    let documents = PushLogBroadcast::decode_gossip_documents(&fixture)
        .expect("a batch")
        .expect("within the cap");
    assert_eq!(documents.len(), 2);
    let (first, first_car) = &documents[0];
    assert_eq!(first.doc_id, "doc-a");
    assert_eq!(first.cid.as_ref(), &[0x01, 0x71, 0xaa]);
    assert_eq!(first.block.as_ref(), &[0xa1]);
    assert_eq!(first.collection_id, "bafy-collection");
    assert_eq!(first.creator, "12D3KooWGo");
    assert!(first_car.is_empty());
    assert_eq!(documents[1].0.doc_id, "doc-b");
    assert_eq!(documents[1].1, Bytes::from_static(&[0x0c]));
}

#[test]
fn single_gossip_decode_leaves_batches_alone() {
    let fixture = bytes_of(GO_PUSH_LOG_BATCH_HEX);
    assert!(PushLogBroadcast::decode_gossip_payload(&fixture).is_err());
}

#[test]
fn gossip_without_batch_or_car_is_not_a_shinzo_payload() {
    let request = PushLogRequest::new(
        "doc".into(),
        Bytes::from_static(&[0x01, 0x71]),
        "col".into(),
        "creator".into(),
        Bytes::from_static(&[0xa1]),
    );
    assert!(PushLogBroadcast::decode_gossip_documents(&encode(&request)).is_none());
}

#[test]
fn oversized_batch_is_refused() {
    let mut request = PushLogRequest::new(
        String::new(),
        Bytes::new(),
        "col".into(),
        "creator".into(),
        Bytes::new(),
    );
    request.documents = vec![p2p::message::DocumentInfo::default(); MAX_INBOUND_DOCUMENTS + 1];
    let decoded = PushLogBroadcast::decode_gossip_documents(&encode(&request)).expect("a batch");
    assert!(decoded.is_err());
}

#[test]
fn signed_messages_are_told_apart_from_raw_cids() {
    assert!(is_cbor_map(&bytes_of(GO_CAR_REQUEST_HEX)));
    let raw = cid::Cid::new_v1(0x71, multihash_of(b"head"));
    assert!(!is_cbor_map(&raw.to_bytes()));
    assert!(!is_cbor_map(&[]));
}

fn multihash_of(data: &[u8]) -> cid::multihash::Multihash<64> {
    use sha2::Digest;
    cid::multihash::Multihash::wrap(0x12, &sha2::Sha256::digest(data)).expect("multihash")
}

#[cfg(feature = "libp2p-transport")]
#[test]
fn signed_request_and_reply_verify() {
    let key = libp2p::identity::Keypair::generate_ed25519();
    let head = cid::Cid::new_v1(0x71, multihash_of(b"head"));
    let mut request = CarRequest::new(&[head]);
    p2p::signing::sign_message(&key, &mut request).expect("sign");
    let decoded: CarRequest = defra_core::cbor::from_slice(&encode(&request)).expect("decode");
    p2p::signing::verify_message(&decoded).expect("request verifies");

    let mut reply = CarReply::new(&request.message_id, vec![vec![0x0a], Vec::new()]);
    p2p::signing::sign_message(&key, &mut reply).expect("sign");
    let mut decoded: CarReply = defra_core::cbor::from_slice(&encode(&reply)).expect("decode");
    p2p::signing::verify_message(&decoded).expect("reply verifies");
    decoded.cars = Some(Vec::new());
    assert!(p2p::signing::verify_message(&decoded).is_err());
}
