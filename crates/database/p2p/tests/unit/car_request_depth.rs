use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};
use serde::{Deserialize, Serialize};

use super::CarFetchRequest;

/// The request as peers built before `max_depth` existed encode and decode it.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct LegacyCarFetchRequest {
    root_cid: Cid,
    #[serde(default)]
    wanted_cids: Vec<Cid>,
    #[serde(default)]
    recursive: bool,
}

fn cid(seed: u8) -> Cid {
    Cid::new_v1(0x71, Code::Sha2_256.digest(&[seed]))
}

fn legacy(wanted_cids: Vec<Cid>) -> LegacyCarFetchRequest {
    LegacyCarFetchRequest {
        root_cid: cid(1),
        wanted_cids,
        recursive: true,
    }
}

#[test]
fn unbounded_request_keeps_its_legacy_bytes() {
    let request = CarFetchRequest::selective_dag(cid(1), vec![cid(2), cid(3)]);

    assert_eq!(
        defra_core::cbor::to_vec(&request).unwrap(),
        defra_core::cbor::to_vec(&legacy(vec![cid(2), cid(3)])).unwrap()
    );
}

#[test]
fn legacy_request_decodes_as_unbounded() {
    let bytes = defra_core::cbor::to_vec(&legacy(vec![cid(1)])).unwrap();

    let decoded: CarFetchRequest = defra_core::cbor::from_slice(&bytes).unwrap();

    assert_eq!(decoded, CarFetchRequest::full_dag(cid(1)));
}

#[test]
fn bounded_request_round_trips_and_legacy_peers_still_read_it() {
    let request = CarFetchRequest {
        max_depth: Some(4),
        ..CarFetchRequest::selective_dag(cid(1), vec![cid(2)])
    };
    let bytes = defra_core::cbor::to_vec(&request).unwrap();

    let decoded: CarFetchRequest = defra_core::cbor::from_slice(&bytes).unwrap();
    assert_eq!(decoded, request);
    let decoded: LegacyCarFetchRequest = defra_core::cbor::from_slice(&bytes).unwrap();
    assert_eq!(decoded, legacy(vec![cid(2)]));
}
