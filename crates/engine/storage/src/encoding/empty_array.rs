//! Empty array marker encoding

use super::{peek_type, EncodedType, EMPTY_ARRAY, EMPTY_ARRAY_DESC};

/// Encode the empty array marker in ascending order
pub fn encode_empty_array_ascending(mut buf: Vec<u8>) -> Vec<u8> {
    buf.push(EMPTY_ARRAY);
    buf
}

/// Encode the empty array marker in descending order
pub fn encode_empty_array_descending(mut buf: Vec<u8>) -> Vec<u8> {
    buf.push(EMPTY_ARRAY_DESC);
    buf
}

/// Decode and check if buffer starts with the empty array marker
pub fn decode_if_empty_array(buf: &[u8]) -> (&[u8], bool) {
    match peek_type(buf) {
        EncodedType::EmptyArray => (&buf[1..], true),
        _ => (buf, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::{encode_null_ascending, encode_null_descending};

    #[test]
    fn empty_array_round_trips() {
        for buf in [
            encode_empty_array_ascending(vec![]),
            encode_empty_array_descending(vec![]),
        ] {
            let (rest, is_empty) = decode_if_empty_array(&buf);
            assert!(is_empty);
            assert!(rest.is_empty());
        }
    }

    #[test]
    fn empty_array_is_not_null() {
        assert_ne!(
            encode_empty_array_ascending(vec![]),
            encode_null_ascending(vec![])
        );
        assert_ne!(
            encode_empty_array_descending(vec![]),
            encode_null_descending(vec![])
        );
        assert_ne!(
            encode_empty_array_ascending(vec![]),
            encode_empty_array_descending(vec![])
        );
    }
}
