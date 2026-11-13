//! The reply sizes `keys_per_request` assumes, measured on real encodings:
//! encryption blocks, their CIDs, ECIES wrapping and the response envelope.

use kms::{wrap_for_requester, FetchEncryptionKeyReply, FetchEncryptionKeyRequest};

use super::*;
use crate::pubsub_rpc::{derive_request_id, InternalResponse};

/// A DID as long as a `did:key` for an Ed25519 node key.
const DID: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

/// The request for `keys` keys, as the requester publishes it.
fn request(keys: usize) -> Vec<u8> {
    let links = (0..keys)
        .map(|index| {
            defra_core::Encryption::new(vec![index as u8; 32])
                .generate_cid()
                .unwrap()
                .to_bytes()
        })
        .collect();
    let request = FetchEncryptionKeyRequest {
        identity: DID.as_bytes().to_vec(),
        links,
        ephemeral_public_key: vec![7; 32],
        explicit_replay_capability: None,
    };
    defra_core::cbor::to_vec(&request).unwrap()
}

/// The reply envelope releasing `keys` keys, as the responder publishes it.
fn reply(keys: usize) -> Vec<u8> {
    let responder = crypto::generate_x25519().unwrap();
    let mut links = Vec::new();
    let mut blocks = Vec::new();
    for index in 0..keys {
        let block = defra_core::Encryption::new(vec![index as u8; 32]);
        links.push(block.generate_cid().unwrap().to_bytes());
        blocks.push(
            wrap_for_requester(
                &block.to_dag_cbor().unwrap(),
                &[9; 32],
                &responder,
                b"associated data",
            )
            .unwrap(),
        );
    }
    let reply = FetchEncryptionKeyReply {
        links,
        blocks,
        // Only its length counts here.
        ephemeral_public_key: vec![9; 32],
    };
    InternalResponse {
        id: derive_request_id(&request(keys)).to_string(),
        err: String::new(),
        data: defra_core::cbor::to_vec(&reply).unwrap(),
        from: Vec::new(),
    }
    .to_cbor()
    .unwrap()
}

/// The constants bound every measured reply, and are no more than a few
/// bytes per key above it, so the limit they give is the real one.
#[test]
fn the_reply_size_bound_is_measured() {
    for keys in [0, 1, 2, 23, 24, 31, 32, 64, 255, 256, 512] {
        let measured = reply(keys).len();
        let bound = REPLY_FIXED + keys * REPLY_PER_KEY;
        eprintln!("keys {keys}: reply {measured} bytes, bound {bound}");
        assert!(measured <= bound, "{keys} keys: {measured} > {bound}");
        assert!(
            bound - measured <= 16 + keys * 2,
            "{keys} keys: bound {bound} is loose over {measured}"
        );
    }
}

/// Over iroh gossip, a request for `keys_per_request` keys and its reply
/// both fit one broadcast, and the reply for one key more would not.
#[test]
#[cfg(feature = "iroh-transport")]
fn a_request_and_its_reply_fit_one_gossip_broadcast() {
    let limit = crate::iroh::MAX_RAW_PAYLOAD;
    let keys = keys_per_request(limit);
    eprintln!(
        "limit {limit}: {keys} keys, reply {} bytes, request {} bytes",
        reply(keys).len(),
        request(keys).len()
    );
    assert!(reply(keys).len() <= limit);
    assert!(request(keys).len() <= limit);
    assert!(
        reply(keys + 1).len() > limit,
        "the limit leaves keys unused"
    );
}
