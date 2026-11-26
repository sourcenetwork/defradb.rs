use proptest::prelude::*;

use super::*;

fn values() -> impl Strategy<Value = Vec<Vec<u8>>> {
    prop::collection::vec(prop::collection::vec(any::<u8>(), 0..300), 0..6)
}

fn frame() -> impl Strategy<Value = Frame> {
    (any::<u32>(), any::<u32>(), 1..u64::MAX)
        .prop_map(|(collection, index, doc)| Frame::new(collection, index, doc))
}

proptest! {
    #[test]
    fn a_record_reads_back_as_its_keys(frame in frame(), values in values()) {
        let keys: Vec<Vec<u8>> = values.iter().map(|values| frame.key(values)).collect();
        prop_assert_eq!(decode_record(&frame, &encode_record(&frame, &keys).unwrap()).unwrap(), keys);
    }

    #[test]
    fn a_record_of_whole_keys_still_reads_back(frame in frame(), keys in values()) {
        prop_assert_eq!(decode_record(&frame, &encode_keys(&keys)).unwrap(), keys);
    }

    #[test]
    fn a_truncated_record_is_refused(frame in frame(), values in values(), cut in 1usize..8) {
        let keys: Vec<Vec<u8>> = values.iter().map(|values| frame.key(values)).collect();
        prop_assume!(!keys.is_empty());
        let bytes = encode_record(&frame, &keys).unwrap();
        let cut = cut.min(bytes.len() - 1);
        if let Ok(decoded) = decode_record(&frame, &bytes[..bytes.len() - cut]) {
            prop_assert_ne!(decoded, keys);
        }
    }

    #[test]
    fn a_record_keeps_only_the_values(frame in frame(), values in values()) {
        let keys: Vec<Vec<u8>> = values.iter().map(|values| frame.key(values)).collect();
        let lengths: usize = values.iter().map(|values| values.len() + if values.len() < 255 { 1 } else { 5 }).sum();
        prop_assert_eq!(encode_record(&frame, &keys).unwrap().len(), 1 + lengths);
    }
}

#[test]
fn a_length_of_255_and_over_takes_four_more_bytes() {
    let frame = Frame::new(3, 1, 9);
    for len in [0, 1, 254, 255, 256, 70_000] {
        let keys = vec![frame.key(&vec![7; len])];
        let bytes = encode_record(&frame, &keys).unwrap();
        let header = if len < 255 { 1 } else { 5 };
        assert_eq!(bytes.len(), 1 + header + len, "values of {len} bytes");
        assert_eq!(decode_record(&frame, &bytes).unwrap(), keys);
    }
}

#[test]
fn a_key_of_another_document_is_refused() {
    let frame = Frame::new(3, 1, 9);
    let other = Frame::new(3, 1, 10).key(b"v");
    assert!(encode_record(&frame, &[other]).is_err());
    let other_index = Frame::new(3, 2, 9).key(b"v");
    assert!(encode_record(&frame, &[other_index]).is_err());
}

#[test]
fn a_record_of_whole_keys_never_starts_as_a_record_of_values() {
    let key = vec![0; 8 * 1024 * 1024];
    assert_eq!(encode_keys(&[key])[0], 0);
    assert!(encode_keys(&[]).is_empty());
}

#[test]
fn a_record_of_neither_form_is_refused() {
    let frame = Frame::new(3, 1, 9);
    assert!(decode_record(&frame, b"bad").is_err());
}
