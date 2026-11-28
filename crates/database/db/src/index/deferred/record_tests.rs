use proptest::prelude::*;

use super::*;

fn values() -> impl Strategy<Value = Vec<Vec<u8>>> {
    prop::collection::vec(prop::collection::vec(any::<u8>(), 0..300), 0..6)
}

fn whole_keys() -> impl Strategy<Value = Vec<Vec<u8>>> {
    prop::collection::vec(prop::collection::vec(any::<u8>(), 1..300), 0..6)
}

fn framing() -> impl Strategy<Value = (u32, u32, u64)> {
    (any::<u32>(), any::<u32>(), 1..u64::MAX)
}

proptest! {
    #[test]
    fn a_record_reads_back_as_its_keys((collection, index, doc) in framing(), values in values()) {
        let prefix = IndexDataStoreKey::index_prefix(collection, index);
        let frame = Frame::new(&prefix, doc);
        let keys: Vec<Vec<u8>> = values.iter().map(|values| frame.key(values)).collect();
        prop_assert_eq!(decode_record(&frame, &encode_record(&frame, &keys).unwrap()).unwrap(), keys);
    }

    #[test]
    fn a_record_of_whole_keys_still_reads_back((collection, index, doc) in framing(), keys in whole_keys()) {
        let prefix = IndexDataStoreKey::index_prefix(collection, index);
        let frame = Frame::new(&prefix, doc);
        prop_assert_eq!(decode_record(&frame, &encode_keys(&keys)).unwrap(), keys);
    }

    #[test]
    fn a_truncated_record_is_refused((collection, index, doc) in framing(), values in values(), cut in 1usize..8) {
        let prefix = IndexDataStoreKey::index_prefix(collection, index);
        let frame = Frame::new(&prefix, doc);
        let keys: Vec<Vec<u8>> = values.iter().map(|values| frame.key(values)).collect();
        prop_assume!(!keys.is_empty());
        let bytes = encode_record(&frame, &keys).unwrap();
        let cut = cut.min(bytes.len() - 1);
        if let Ok(decoded) = decode_record(&frame, &bytes[..bytes.len() - cut]) {
            prop_assert_ne!(decoded, keys);
        }
    }

    #[test]
    fn a_record_keeps_only_the_values((collection, index, doc) in framing(), values in values()) {
        let prefix = IndexDataStoreKey::index_prefix(collection, index);
        let frame = Frame::new(&prefix, doc);
        let keys: Vec<Vec<u8>> = values.iter().map(|values| frame.key(values)).collect();
        let lengths: usize = values.iter().map(|values| values.len() + if values.len() < 255 { 1 } else { 5 }).sum();
        prop_assert_eq!(encode_record(&frame, &keys).unwrap().len(), VALUES.len() + lengths);
    }
}

#[test]
fn a_length_of_255_and_over_takes_four_more_bytes() {
    let prefix = IndexDataStoreKey::index_prefix(3, 1);
    let frame = Frame::new(&prefix, 9);
    for len in [0, 1, 254, 255, 256, 70_000] {
        let keys = vec![frame.key(&vec![7; len])];
        let bytes = encode_record(&frame, &keys).unwrap();
        let header = if len < 255 { 1 } else { 5 };
        assert_eq!(
            bytes.len(),
            VALUES.len() + header + len,
            "values of {len} bytes"
        );
        assert_eq!(decode_record(&frame, &bytes).unwrap(), keys);
    }
}

#[test]
fn a_key_of_another_document_is_refused() {
    let prefix = IndexDataStoreKey::index_prefix(3, 1);
    let frame = Frame::new(&prefix, 9);
    let other = Frame::new(&prefix, 10).key(b"v");
    assert!(encode_record(&frame, &[other]).is_err());
    let other_prefix = IndexDataStoreKey::index_prefix(3, 2);
    let other_index = Frame::new(&other_prefix, 9).key(b"v");
    assert!(encode_record(&frame, &[other_index]).is_err());
}

#[test]
fn a_large_whole_key_record_is_not_taken_for_values() {
    let prefix = IndexDataStoreKey::index_prefix(3, 1);
    let frame = Frame::new(&prefix, 9);
    let key = vec![0; 16 * 1024 * 1024];
    let bytes = encode_keys(std::slice::from_ref(&key));
    assert_eq!(
        bytes[..4],
        [1, 0, 0, 0],
        "a legacy length can start with one"
    );
    assert_eq!(decode_record(&frame, &bytes).unwrap(), [key]);
    assert!(encode_keys(&[]).is_empty());
}

#[test]
fn a_record_of_neither_form_is_refused() {
    let prefix = IndexDataStoreKey::index_prefix(3, 1);
    let frame = Frame::new(&prefix, 9);
    assert!(decode_record(&frame, b"bad").is_err());
}
