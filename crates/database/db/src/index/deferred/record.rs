//! A position record: the entries a deferred index holds for one document
//! (`proofs/lean/PositionRecord`).
//!
//! The record's own key names the collection, the index and the document, so
//! it keeps each entry as its field values alone: the bytes between the
//! index's prefix and the document's suffix. A record written before kept
//! whole keys; it still reads back, and the next write of it keeps values.

use storage::keys::IndexDataStoreKey;

use crate::index::{Error, Result};

/// The first byte of a record that keeps field values. A record of whole keys
/// is empty or starts with a zero byte, the top byte of its first key's
/// four-byte length, because regolith refuses keys of 16 MiB and over.
const VALUES: u8 = 1;

/// A length byte that says the length follows in four big-endian bytes.
const LONG: u8 = u8::MAX;

/// What every entry key of one document in one index starts and ends with.
#[derive(Debug)]
pub(crate) struct Frame {
    prefix: Vec<u8>,
    suffix: Vec<u8>,
}

impl Frame {
    pub(crate) fn new(collection_short_id: u32, index_id: u32, doc_short_id: u64) -> Self {
        Self {
            prefix: IndexDataStoreKey::index_prefix(collection_short_id, index_id),
            suffix: IndexDataStoreKey::doc_suffix(doc_short_id),
        }
    }

    fn values<'k>(&self, key: &'k [u8]) -> Option<&'k [u8]> {
        key.strip_prefix(self.prefix.as_slice())?
            .strip_suffix(self.suffix.as_slice())
    }

    fn key(&self, values: &[u8]) -> Vec<u8> {
        [self.prefix.as_slice(), values, self.suffix.as_slice()].concat()
    }
}

/// [`VALUES`], then each key's field values behind their length: one byte
/// below [`LONG`], else [`LONG`] and four big-endian bytes.
pub(crate) fn encode_record(frame: &Frame, keys: &[Vec<u8>]) -> Result<Vec<u8>> {
    let framing = frame.prefix.len() + frame.suffix.len();
    let mut out = Vec::with_capacity(
        1 + keys
            .iter()
            .map(|key| key.len().saturating_sub(framing) + 1)
            .sum::<usize>(),
    );
    out.push(VALUES);
    for key in keys {
        let values = frame.values(key).ok_or_else(|| {
            Error::Other("index entry key is not one of its document's entries".into())
        })?;
        push_len(&mut out, values.len())?;
        out.extend_from_slice(values);
    }
    Ok(out)
}

/// The entry keys a record names, in either form.
pub(crate) fn decode_record(frame: &Frame, bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let Some((&VALUES, mut rest)) = bytes.split_first() else {
        return decode_keys(bytes);
    };
    let mut keys = Vec::new();
    while !rest.is_empty() {
        let (values, after) = take(rest, read_len)?;
        keys.push(frame.key(values));
        rest = after;
    }
    Ok(keys)
}

/// A record of whole keys, each behind its length in four big-endian bytes.
fn decode_keys(mut bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut keys = Vec::new();
    while !bytes.is_empty() {
        let (key, rest) = take(bytes, read_be32)?;
        keys.push(key.to_vec());
        bytes = rest;
    }
    Ok(keys)
}

fn push_len(out: &mut Vec<u8>, len: usize) -> Result<()> {
    match u8::try_from(len) {
        Ok(short) if short < LONG => out.push(short),
        _ => {
            let len = u32::try_from(len)
                .map_err(|_| Error::Other("index entry values longer than 4 GiB".into()))?;
            out.push(LONG);
            out.extend_from_slice(&len.to_be_bytes());
        }
    }
    Ok(())
}

fn read_len(bytes: &[u8]) -> Option<(usize, &[u8])> {
    match bytes.split_first()? {
        (&LONG, rest) => read_be32(rest),
        (&short, rest) => Some((usize::from(short), rest)),
    }
}

fn read_be32(bytes: &[u8]) -> Option<(usize, &[u8])> {
    let (len, rest) = bytes.split_first_chunk::<4>()?;
    Some((u32::from_be_bytes(*len) as usize, rest))
}

/// A length `read` from the front of `bytes`, then that many bytes, and what
/// follows them.
fn take<'b, F>(bytes: &'b [u8], read: F) -> Result<(&'b [u8], &'b [u8])>
where
    F: Fn(&'b [u8]) -> Option<(usize, &'b [u8])>,
{
    read(bytes)
        .filter(|(len, rest)| *len <= rest.len())
        .map(|(len, rest)| rest.split_at(len))
        .ok_or_else(|| Error::Other("position record cut short".into()))
}

/// A record of whole keys, as a store written before records kept field
/// values holds it.
#[cfg(test)]
pub(crate) fn encode_keys(keys: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for key in keys {
        out.extend_from_slice(&u32::try_from(key.len()).unwrap().to_be_bytes());
        out.extend_from_slice(key);
    }
    out
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
