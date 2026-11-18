use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};
use serde::{Deserialize, Serialize};

use super::CarFetchRequest;

/// The request as peers built before `have_cids` existed encode and decode it.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct PreHavesRequest {
    root_cid: Cid,
    #[serde(default)]
    wanted_cids: Vec<Cid>,
    #[serde(default)]
    recursive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_depth: Option<u32>,
}

fn cid(seed: u8) -> Cid {
    Cid::new_v1(0x71, Code::Sha2_256.digest(&[seed]))
}

#[test]
fn a_request_without_haves_keeps_its_bytes() {
    let request = CarFetchRequest {
        max_depth: Some(2),
        ..CarFetchRequest::selective_dag(cid(1), vec![cid(2)])
    };
    let before = PreHavesRequest {
        root_cid: cid(1),
        wanted_cids: vec![cid(2)],
        recursive: true,
        max_depth: Some(2),
    };

    assert_eq!(
        defra_core::cbor::to_vec(&request).unwrap(),
        defra_core::cbor::to_vec(&before).unwrap()
    );
}

#[test]
fn a_request_with_haves_round_trips_and_older_peers_read_the_rest() {
    let request = CarFetchRequest {
        max_depth: Some(2),
        ..CarFetchRequest::selective_dag(cid(1), vec![cid(2)])
    }
    .with_haves(vec![cid(3), cid(4), cid(3)]);
    assert_eq!(request.have_cids, vec![cid(3), cid(4)]);
    let bytes = defra_core::cbor::to_vec(&request).unwrap();

    let decoded: CarFetchRequest = defra_core::cbor::from_slice(&bytes).unwrap();
    assert_eq!(decoded, request);
    let older: PreHavesRequest = defra_core::cbor::from_slice(&bytes).unwrap();
    assert_eq!(older.wanted_cids, vec![cid(2)]);
    assert_eq!(older.max_depth, Some(2));
}
