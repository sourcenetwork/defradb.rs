mod common;

use bitswap::pb;
use bitswap::{BitswapMessage, Block, BlockPresence, Error, WantType};
use bytes::Bytes;
use cid::Cid;
use common::{block_v0, block_v1, cid_v1};
use prost::Message;

fn entry(m: &BitswapMessage, cid: &Cid) -> bitswap::Entry {
    m.wantlist().find(|e| &e.cid == cid).cloned().unwrap()
}

fn roundtrip(m: &BitswapMessage) -> BitswapMessage {
    BitswapMessage::try_from(Bytes::from(m.encode_as_proto_v1().unwrap().encode_to_vec())).unwrap()
}

#[test]
fn new_entry_returns_encoded_len_and_merge_returns_zero() {
    let mut m = BitswapMessage::new(false);
    let c = cid_v1(b"a");
    let size = m.add_entry(c, 5, WantType::Block, false);
    assert_eq!(size, entry(&m, &c).encoded_len());
    assert!(size > 0);
    assert_eq!(m.add_entry(c, 7, WantType::Block, false), 0);
}

#[test]
fn priority_changes_only_for_same_want_type() {
    let mut m = BitswapMessage::new(false);
    let c = cid_v1(b"a");
    m.add_entry(c, 5, WantType::Have, false);
    m.add_entry(c, 9, WantType::Have, false);
    assert_eq!(entry(&m, &c).priority, 9);
    m.add_entry(c, 3, WantType::Block, false);
    let e = entry(&m, &c);
    assert_eq!(e.priority, 9);
    assert_eq!(e.want_type, WantType::Block);
}

#[test]
fn block_overrides_have_but_not_reverse() {
    let mut m = BitswapMessage::new(false);
    let c = cid_v1(b"a");
    m.add_entry(c, 1, WantType::Block, false);
    m.add_entry(c, 2, WantType::Have, false);
    let e = entry(&m, &c);
    assert_eq!(e.want_type, WantType::Block);
    assert_eq!(e.priority, 1);
}

#[test]
fn cancel_and_send_dont_have_only_go_false_to_true() {
    let mut m = BitswapMessage::new(false);
    let c = cid_v1(b"a");
    m.add_entry(c, 1, WantType::Block, true);
    m.add_entry(c, 1, WantType::Block, false);
    assert!(entry(&m, &c).send_dont_have);
    assert!(!entry(&m, &c).cancel);
    assert_eq!(m.cancel(c), 0);
    assert!(entry(&m, &c).cancel);
    m.add_entry(c, 1, WantType::Block, false);
    assert!(entry(&m, &c).cancel);
}

#[test]
fn cancel_on_new_cid_is_priority_zero_block_cancel() {
    let mut m = BitswapMessage::new(false);
    let c = cid_v1(b"a");
    assert!(m.cancel(c) > 0);
    let e = entry(&m, &c);
    assert_eq!(
        (e.priority, e.want_type, e.cancel, e.send_dont_have),
        (0, WantType::Block, true, false)
    );
}

#[test]
fn remove_drops_entry() {
    let mut m = BitswapMessage::new(false);
    let c = cid_v1(b"a");
    m.add_entry(c, 1, WantType::Block, false);
    m.remove(&c);
    assert!(m.is_empty());
}

#[test]
fn block_and_presence_interplay() {
    let mut m = BitswapMessage::new(false);
    let b = block_v1(b"x");
    let c = *b.cid();
    m.add_have(c);
    assert_eq!(m.haves().count(), 1);
    m.add_block(b);
    assert_eq!(m.block_presences().count(), 0);
    m.add_dont_have(c);
    assert_eq!(m.block_presences().count(), 0);
    assert_eq!(m.blocks_len(), 1);

    let other = cid_v1(b"y");
    m.add_dont_have(other);
    assert_eq!(m.dont_haves().collect::<Vec<_>>(), vec![&other]);
    m.add_have(other);
    assert_eq!(m.dont_haves().count(), 0);
    assert_eq!(m.haves().count(), 1);
}

#[test]
fn clear_resets_everything() {
    let mut m = BitswapMessage::new(false);
    m.add_entry(cid_v1(b"a"), 1, WantType::Block, false);
    m.add_block(block_v1(b"b"));
    m.add_have(cid_v1(b"c"));
    m.set_pending_bytes(9);
    m.clear(true);
    assert!(m.full());
    assert!(m.is_empty());
    assert_eq!(m.pending_bytes(), 0);
}

#[test]
fn encoded_len_is_sum_of_parts() {
    let mut m = BitswapMessage::new(false);
    let c = cid_v1(b"a");
    let e = m.add_entry(c, 1, WantType::Block, false);
    let b = block_v1(b"hello");
    m.add_block(b);
    let have = cid_v1(b"h");
    m.add_have(have);
    assert_eq!(
        m.encoded_len(),
        e + 5 + BlockPresence::encoded_len_for_cid(have)
    );
}

#[test]
fn v0_encoding_has_wantlist_and_raw_blocks_only() {
    let mut m = BitswapMessage::new(true);
    m.add_block(block_v0(b"zz"));
    m.add_have(cid_v1(b"h"));
    m.set_pending_bytes(4);
    let pbm = m.encode_as_proto_v0();
    let wl = pbm.wantlist.unwrap();
    assert!(wl.full);
    assert_eq!(pbm.blocks, vec![Bytes::from_static(b"zz")]);
    assert!(pbm.payload.is_empty());
    assert!(pbm.block_presences.is_empty());
    assert_eq!(pbm.pending_bytes, 0);
}

#[test]
fn v1_encoding_has_payload_presences_and_pending() {
    let mut m = BitswapMessage::new(false);
    let b = block_v1(b"zz");
    m.add_block(b.clone());
    m.add_have(cid_v1(b"h"));
    m.set_pending_bytes(4);
    let pbm = m.encode_as_proto_v1().unwrap();
    assert!(pbm.wantlist.is_some());
    assert!(pbm.blocks.is_empty());
    assert_eq!(pbm.payload.len(), 1);
    assert_eq!(pbm.payload[0].data, b.data().clone());
    assert_eq!(pbm.block_presences.len(), 1);
    assert_eq!(pbm.pending_bytes, 4);
}

#[test]
fn empty_message_always_encodes_an_empty_wantlist() {
    let m = BitswapMessage::new(false);
    assert_eq!(m.encode_as_proto_v0().encode_to_vec(), vec![0x0a, 0x00]);
    assert_eq!(
        m.encode_as_proto_v1().unwrap().encode_to_vec(),
        vec![0x0a, 0x00]
    );
}

#[test]
fn v1_roundtrip_every_shape() {
    let mut m = BitswapMessage::new(true);
    m.add_entry(cid_v1(b"w1"), 3, WantType::Have, true);
    m.add_entry(cid_v1(b"w2"), 1, WantType::Block, false);
    m.cancel(cid_v1(b"w3"));
    m.add_block(block_v1(b"b1"));
    m.add_block(block_v0(b"b0"));
    m.add_have(cid_v1(b"h"));
    m.add_dont_have(cid_v1(b"d"));
    m.set_pending_bytes(77);
    assert_eq!(roundtrip(&m), m);
}

#[test]
fn v0_roundtrip_yields_cidv0_blocks() {
    let mut m = BitswapMessage::new(false);
    m.add_block(block_v0(b"legacy"));
    m.add_entry(cid_v1(b"w"), 2, WantType::Block, false);
    let bytes = Bytes::from(m.encode_as_proto_v0().encode_to_vec());
    assert_eq!(BitswapMessage::try_from(bytes).unwrap(), m);
}

#[test]
fn empty_protobuf_decodes_to_empty_message() {
    let m = BitswapMessage::try_from(Bytes::new()).unwrap();
    assert!(m.is_empty());
    assert!(!m.full());
    assert_eq!(m.pending_bytes(), 0);
}

#[test]
fn decode_merges_duplicate_wantlist_entries() {
    let c = cid_v1(b"a");
    let e = |cancel, sdh| pb::message::wantlist::Entry {
        block: c.to_bytes(),
        priority: 4,
        cancel,
        want_type: 0,
        send_dont_have: sdh,
    };
    let pbm = pb::Message {
        wantlist: Some(pb::message::Wantlist {
            entries: vec![e(false, false), e(true, true)],
            full: false,
        }),
        ..Default::default()
    };
    let m = BitswapMessage::try_from(pbm).unwrap();
    let got = entry(&m, &c);
    assert!(got.cancel && got.send_dont_have);
}

#[test]
fn decode_drops_presence_for_present_block() {
    let b = block_v1(b"x");
    let pbm = pb::Message {
        payload: vec![pb::message::Block {
            prefix: bitswap::Prefix::try_from(b.cid()).unwrap().to_bytes(),
            data: b.data().clone(),
        }],
        block_presences: vec![pb::message::BlockPresence {
            cid: b.cid().to_bytes(),
            r#type: 1,
        }],
        ..Default::default()
    };
    let m = BitswapMessage::try_from(pbm).unwrap();
    assert_eq!(m.blocks_len(), 1);
    assert_eq!(m.block_presences().count(), 0);
}

#[test]
fn invalid_want_type_fails_the_message() {
    let pbm = pb::Message {
        wantlist: Some(pb::message::Wantlist {
            entries: vec![pb::message::wantlist::Entry {
                block: cid_v1(b"a").to_bytes(),
                want_type: 7,
                ..Default::default()
            }],
            full: false,
        }),
        ..Default::default()
    };
    assert!(matches!(
        BitswapMessage::try_from(pbm),
        Err(Error::InvalidWantType(7))
    ));
}

#[test]
fn invalid_presence_type_fails_the_message() {
    let pbm = pb::Message {
        block_presences: vec![pb::message::BlockPresence {
            cid: cid_v1(b"a").to_bytes(),
            r#type: 5,
        }],
        ..Default::default()
    };
    assert!(matches!(
        BitswapMessage::try_from(pbm),
        Err(Error::InvalidBlockPresenceType(5))
    ));
}

#[test]
fn invalid_cid_bytes_fail() {
    let pbm = pb::Message {
        wantlist: Some(pb::message::Wantlist {
            entries: vec![pb::message::wantlist::Entry {
                block: vec![0xff, 0xff],
                ..Default::default()
            }],
            full: false,
        }),
        ..Default::default()
    };
    assert!(matches!(BitswapMessage::try_from(pbm), Err(Error::Cid(_))));
    let pbm = pb::Message {
        block_presences: vec![pb::message::BlockPresence {
            cid: vec![1],
            r#type: 0,
        }],
        ..Default::default()
    };
    assert!(BitswapMessage::try_from(pbm).is_err());
}

#[test]
fn garbage_protobuf_is_a_decode_error() {
    assert!(matches!(
        BitswapMessage::try_from(Bytes::from_static(&[0x0a, 0x05, 0x01])),
        Err(Error::Protobuf(_))
    ));
}

#[test]
fn unknown_prefix_hash_is_an_error() {
    let pbm = pb::Message {
        payload: vec![pb::message::Block {
            prefix: vec![1, 0x55, 0x7f, 32],
            data: Bytes::from_static(b"x"),
        }],
        ..Default::default()
    };
    assert!(matches!(
        BitswapMessage::try_from(pbm),
        Err(Error::UnsupportedMultihashCode(0x7f))
    ));
}

#[test]
fn verify_blocks_drops_mismatched_data() {
    let good = block_v1(b"good");
    let bad = Block::new(Bytes::from_static(b"tampered"), cid_v1(b"original"));
    let mut m = BitswapMessage::new(false);
    m.add_block(good.clone());
    m.add_block(bad);
    m.verify_blocks();
    assert_eq!(m.blocks().collect::<Vec<_>>(), vec![&good]);
}

#[test]
fn prefix_roundtrips_and_rejects_unknown_hash() {
    let c = cid_v1(b"p");
    let p = bitswap::Prefix::try_from(&c).unwrap();
    assert_eq!(bitswap::Prefix::new(&p.to_bytes()).unwrap(), p);
    assert_eq!(p.to_cid(b"p").unwrap(), c);

    let unknown = Cid::new_v1(
        0x55,
        multihash::Multihash::<64>::wrap(0x7f, &[0u8; 8]).unwrap(),
    );
    assert!(matches!(
        bitswap::Prefix::try_from(&unknown),
        Err(Error::UnsupportedMultihashCode(0x7f))
    ));
}
