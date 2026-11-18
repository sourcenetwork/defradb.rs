use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

use super::{decode_car_request, encode_car_request};
use crate::message::HeldCids;
use crate::sync::car::{MAX_HAVE_CIDS, MAX_PRESENT_CIDS};

fn cid(seed: u8) -> Cid {
    Cid::new_v1(0x71, Code::Sha2_256.digest(&[seed]))
}

fn held(haves: Vec<Cid>, present: Vec<Cid>) -> HeldCids {
    HeldCids { haves, present }
}

#[test]
fn a_request_without_haves_is_the_root_bytes_alone() {
    assert_eq!(
        encode_car_request(&cid(1), &HeldCids::default()),
        cid(1).to_bytes()
    );
    assert_eq!(
        decode_car_request(&cid(1).to_bytes()).unwrap(),
        (cid(1), HeldCids::default())
    );
}

#[test]
fn haves_round_trip_and_an_older_responder_reads_the_root() {
    let bytes = encode_car_request(&cid(1), &held(vec![cid(2), cid(3)], vec![]));

    assert_eq!(
        decode_car_request(&bytes).unwrap(),
        (cid(1), held(vec![cid(2), cid(3)], vec![]))
    );
    let older = Cid::read_bytes(std::io::Cursor::new(&bytes)).unwrap();
    assert_eq!(older, cid(1));
}

#[test]
fn haves_are_capped_and_a_torn_tail_is_ignored() {
    let many: Vec<Cid> = (2..40).map(cid).collect();
    let mut bytes = encode_car_request(&cid(1), &held(many.clone(), vec![]));
    let (_, decoded) = decode_car_request(&bytes).unwrap();
    assert_eq!(decoded.haves, many[..MAX_HAVE_CIDS].to_vec());

    bytes.truncate(cid(1).to_bytes().len() + 5);
    assert_eq!(
        decode_car_request(&bytes).unwrap(),
        (cid(1), HeldCids::default())
    );
}

#[test]
fn present_cids_follow_the_marker_with_or_without_haves() {
    for haves in [vec![], vec![cid(2)]] {
        let request = held(haves, vec![cid(3), cid(4)]);
        let bytes = encode_car_request(&cid(1), &request);
        assert_eq!(decode_car_request(&bytes).unwrap(), (cid(1), request));
    }
}

#[test]
fn present_cids_are_capped() {
    let many: Vec<Cid> = (2..=(MAX_PRESENT_CIDS as u8 + 10)).map(cid).collect();
    let bytes = encode_car_request(&cid(1), &held(vec![], many.clone()));
    let (_, decoded) = decode_car_request(&bytes).unwrap();
    assert_eq!(decoded.present, many[..MAX_PRESENT_CIDS].to_vec());
}
