//! Go Shinzo's datastore and headstore keys, as a snapshot carries them.
//! Their integers use Go's ascending uvarint (`storage::encoding`), not the
//! encoding of defradb.rs's own keys.

use cid::Cid;
use storage::encoding::{decode_uvarint_ascending, encode_uvarint_ascending};

/// A datastore instance: a value, a priority or a deletion marker.
#[derive(Debug, Clone, Copy)]
pub enum Instance {
    Value,
    Priority,
}

impl Instance {
    fn byte(self) -> u8 {
        match self {
            Instance::Value => b'v',
            Instance::Priority => b'p',
        }
    }
}

/// `/<collection short ID>/<instance>/<doc short ID>/<field>`.
pub fn datastore_key(collection: u32, instance: Instance, doc: u64, field: &str) -> Vec<u8> {
    let mut key = encode_uvarint_ascending(vec![b'/'], u64::from(collection));
    key.extend_from_slice(&[b'/', instance.byte(), b'/']);
    let mut key = encode_uvarint_ascending(key, doc);
    key.push(b'/');
    key.extend_from_slice(field.as_bytes());
    key
}

/// `/d/<doc short ID>/<field>/<CID text>`.
pub fn headstore_key(doc: u64, field: &str, cid: &Cid) -> Vec<u8> {
    let mut key = encode_uvarint_ascending(b"/d/".to_vec(), doc);
    key.push(b'/');
    key.extend_from_slice(field.as_bytes());
    key.push(b'/');
    key.extend_from_slice(cid.to_string().as_bytes());
    key
}

/// The document short ID a datastore key names.
pub fn datastore_doc(key: &[u8]) -> Option<u64> {
    let rest = key.strip_prefix(b"/")?;
    let (rest, _) = decode_uvarint_ascending(rest).ok()?;
    let rest = rest.strip_prefix(b"/")?.get(1..)?.strip_prefix(b"/")?;
    let (rest, doc) = decode_uvarint_ascending(rest).ok()?;
    rest.starts_with(b"/").then_some(doc)
}

/// The document short ID, field and CID a headstore key names.
pub fn headstore_head(key: &[u8]) -> Option<(u64, &str, Cid)> {
    let rest = key.strip_prefix(b"/d/")?;
    let (rest, doc) = decode_uvarint_ascending(rest).ok()?;
    let (field, cid) = std::str::from_utf8(rest.strip_prefix(b"/")?)
        .ok()?
        .split_once('/')?;
    Some((doc, field, cid.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_match_go_shinzo_bytes() {
        // Go Shinzo 2a899b860's export of a first document of a first collection.
        assert_eq!(datastore_key(1, Instance::Value, 1, "2"), b"/\x89/v/\x89/2");
        assert_eq!(
            datastore_key(1, Instance::Priority, 1, "3"),
            b"/\x89/p/\x89/3"
        );
        let cid: Cid = "bafyreifk2n33xqphceptysxhvk4mci3nb46p5pkymxpbmclfswhpp6wvnu"
            .parse()
            .unwrap();
        let head = headstore_key(1, "C", &cid);
        assert_eq!(
            head,
            b"/d/\x89/C/bafyreifk2n33xqphceptysxhvk4mci3nb46p5pkymxpbmclfswhpp6wvnu"
        );
        assert_eq!(headstore_head(&head), Some((1, "C", cid)));
        let key = datastore_key(7, Instance::Value, 70_000, "v");
        assert_eq!(datastore_doc(&key), Some(70_000));
    }
}
