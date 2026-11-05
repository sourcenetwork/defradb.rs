//! Go Shinzo's datastore and headstore keys, as a snapshot carries them.
//! Their integers use Go's ascending uvarint (`storage::encoding`), not the
//! encoding of defradb.rs's own keys.

use cid::Cid;
use storage::encoding::decode_uvarint_ascending;

/// The document short ID a datastore key
/// (`/<collection short ID>/<instance>/<doc short ID>/<field>`) names.
pub fn datastore_doc(key: &[u8]) -> Option<u64> {
    let rest = key.strip_prefix(b"/")?;
    let (rest, _) = decode_uvarint_ascending(rest).ok()?;
    let rest = rest.strip_prefix(b"/")?.get(1..)?.strip_prefix(b"/")?;
    let (rest, doc) = decode_uvarint_ascending(rest).ok()?;
    rest.starts_with(b"/").then_some(doc)
}

/// The document short ID, field and CID a headstore key
/// (`/d/<doc short ID>/<field>/<CID text>`) names.
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
    fn keys_read_as_go_shinzo_writes_them() {
        // Go Shinzo 2a899b860's export of a first document of a first collection.
        assert_eq!(datastore_doc(b"/\x89/v/\x89/2"), Some(1));
        assert_eq!(datastore_doc(b"/\x89/p/\x89/3"), Some(1));
        let cid: Cid = "bafyreifk2n33xqphceptysxhvk4mci3nb46p5pkymxpbmclfswhpp6wvnu"
            .parse()
            .unwrap();
        assert_eq!(
            headstore_head(
                b"/d/\x89/C/bafyreifk2n33xqphceptysxhvk4mci3nb46p5pkymxpbmclfswhpp6wvnu"
            ),
            Some((1, "C", cid))
        );
        assert_eq!(datastore_doc(b"/\x89/v/\x89"), None);
    }
}
