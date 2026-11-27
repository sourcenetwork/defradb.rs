//! A position record: the exact keys of the entries a deferred index holds
//! for one document (`proofs/lean/PositionRecord`).

use crate::index::{Error, Result};

/// Each key as its length in four big-endian bytes, then the key.
pub(crate) fn encode_record(keys: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(keys.iter().map(|key| key.len() + 4).sum());
    for key in keys {
        let len = u32::try_from(key.len())
            .map_err(|_| Error::Other("index entry key longer than 4 GiB".into()))?;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(key);
    }
    Ok(out)
}

pub(crate) fn decode_record(mut bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut keys = Vec::new();
    while !bytes.is_empty() {
        let (len, rest) = bytes
            .split_first_chunk::<4>()
            .ok_or_else(|| Error::Other("position record cut short".into()))?;
        let len = u32::from_be_bytes(*len) as usize;
        if rest.len() < len {
            return Err(Error::Other("position record cut short".into()));
        }
        let (key, rest) = rest.split_at(len);
        keys.push(key.to_vec());
        bytes = rest;
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn a_record_reads_back_as_its_keys(keys in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..40), 0..6)) {
            prop_assert_eq!(decode_record(&encode_record(&keys).unwrap()).unwrap(), keys);
        }

        #[test]
        fn a_truncated_record_is_refused(keys in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..40), 1..6), cut in 1usize..8) {
            let bytes = encode_record(&keys).unwrap();
            let cut = cut.min(bytes.len());
            if let Ok(decoded) = decode_record(&bytes[..bytes.len() - cut]) {
                prop_assert_ne!(decoded, keys);
            }
        }
    }
}
