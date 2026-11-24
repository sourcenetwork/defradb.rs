use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

use super::{decode_car_request, encode_car_request};
use crate::sync::car::MAX_HAVE_CIDS;

fn cid(seed: u8) -> Cid {
    Cid::new_v1(0x71, Code::Sha2_256.digest(&[seed]))
}

#[test]
fn a_request_without_haves_is_the_root_bytes_alone() {
    assert_eq!(encode_car_request(&cid(1), &[]), cid(1).to_bytes());
    assert_eq!(
        decode_car_request(&cid(1).to_bytes()).unwrap(),
        (cid(1), vec![])
    );
}

#[test]
fn haves_round_trip_and_an_older_responder_reads_the_root() {
    let bytes = encode_car_request(&cid(1), &[cid(2), cid(3)]);

    assert_eq!(
        decode_car_request(&bytes).unwrap(),
        (cid(1), vec![cid(2), cid(3)])
    );
    let older = Cid::read_bytes(std::io::Cursor::new(&bytes)).unwrap();
    assert_eq!(older, cid(1));
}

#[test]
fn haves_are_capped_and_a_torn_tail_is_ignored() {
    let many: Vec<Cid> = (2..40).map(cid).collect();
    let mut bytes = encode_car_request(&cid(1), &many);
    let (_, haves) = decode_car_request(&bytes).unwrap();
    assert_eq!(haves, many[..MAX_HAVE_CIDS].to_vec());

    bytes.truncate(cid(1).to_bytes().len() + 5);
    assert_eq!(decode_car_request(&bytes).unwrap(), (cid(1), vec![]));
}
