//! The family table against the keys the store writes.

use proptest::prelude::*;

use super::{entity_len, family_of, keys, Class, FAMILIES};
use crate::keys::utils::InstanceType;
use crate::keys::{
    deleted_doc_key, doc_key, encode_doc_short_id, DataStoreKey, HeadstorePriorityKey,
    IndexDataStoreKey,
};

/// `namespace` and a key's bytes, as a store namespace writes them.
fn stored(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    [namespace, key].concat()
}

/// Each family's entity ends where the code that builds its keys ends one:
/// at the prefix that code scans the entity by.
#[test]
fn every_entity_ends_where_its_key_builder_ends_it() {
    let mut entities: Vec<Vec<u8>> = Vec::new();
    for id in [1u64, 47, 239, 240, 287, 2287, 2288, u64::MAX] {
        let document = encode_doc_short_id(id);
        entities.push(stored(b"h", &HeadstorePriorityKey::document_prefix(id)));
        entities.push([b"h/r/".as_slice(), &document, b"/"].concat());
    }
    for id in [0u32, 7, 47, 300, u32::MAX] {
        entities.push(stored(b"d", &IndexDataStoreKey::index_prefix(id, 47)));
        entities.push(stored(b"d", &IndexDataStoreKey::index_prefix(7, id)));
        entities.push(stored(
            b"d",
            &DataStoreKey::collection_instance_prefix(id, InstanceType::Value),
        ));
    }
    for collection in ["bafkreicollection", "users"] {
        for key in [doc_key(collection, 9), deleted_doc_key(collection, 9)] {
            let key = stored(b"d", &key);
            entities.push(key[..key.len() - encode_doc_short_id(9).len()].to_vec());
        }
        entities.push(format!("d/v/{collection}/").into_bytes());
    }
    entities.push(b"a/zanzibar/7/rel/users/bae-1/".to_vec());
    entities.push(b"a/acp/users/bae-1/".to_vec());
    for entity in entities {
        for rest in [b"".as_slice(), b"field/cid", b"\x01\xff/"] {
            assert_eq!(
                entity_len(&[entity.as_slice(), rest].concat()),
                Some(entity.len()),
                "{entity:?}"
            );
        }
    }
}

/// A family's start decides its class, and the first family a key starts
/// with is the key's family.
#[test]
fn the_first_family_a_key_starts_with_is_its_family() {
    let cases: [(&[u8], Class); 14] = [
        (b"bm\x01\x55", Class::Ordinary),
        (b"b\x01\x55", Class::ContentAddressed),
        (b"s/arrival/7/head", Class::Log),
        (b"h/hd/\x01", Class::Ordinary),
        (b"h/hc/1", Class::Ordinary),
        (b"h/rc/1/cid", Class::Ordinary),
        (b"h/p/\x01/", Class::Ordinary),
        (b"h/r/\x01/", Class::Ordinary),
        (b"d/d/users/", Class::Ordinary),
        (b"d/del/users/", Class::Ordinary),
        (b"d/v/users/", Class::Ordinary),
        (b"d/\x01/\x02/", Class::Ordinary),
        (b"a/zanzibar/7/rel/users/bae-1/", Class::Ordinary),
        (b"a/acp/users/bae-1/", Class::Ordinary),
    ];
    for (key, class) in cases {
        assert_eq!(
            family_of(key).map(|family| family.class()),
            Some(class),
            "{key:?}"
        );
    }
    // `d/del/` is the deletion markers' family, not an index of collection
    // `d`: its entity is the collection's name.
    assert_eq!(entity_len(b"d/del/users/\x01"), Some(b"d/del/users/".len()));
    for key in [
        b"".as_slice(),
        b"e\x01\x55",
        b"h/f/users/email/cid",
        b"s/seq/doc",
    ] {
        assert!(family_of(key).is_none(), "{key:?}");
    }
}

/// Blocks, block merge markers and the arrival journal are read by key, never
/// by entity; and a key cut short or malformed names no entity.
#[test]
fn keys_without_an_entity_name_none() {
    for key in [
        b"b\x01\x55\x00\x03abc".as_slice(),
        b"bm\x01\x55\x00\x03abc",
        b"s/arrival/7/row/00000000000000000001",
        b"h/p/",
        b"h/p/\x00/field/cid",
        b"h/p/\xf0",
        b"h/p/\x01",
        b"h/p/\x01x/",
        b"h/hd/\x01",
        b"h/hc/1",
        b"h/rc/1/cid",
        b"d/d/",
        b"d/d//\x01",
        b"d/",
        b"d/\x01",
        b"d/\x01/\xf9",
        b"a/zanzibar/policy/7",
        b"a/zanzibar/counter",
        b"a/zanzibar/7/rel/users/",
        b"a/acp/users/",
    ] {
        assert_eq!(entity_len(key), None, "{key:?}");
    }
}

proptest! {
    /// What both readers of the table rely on: an entity found in a prefix
    /// of a key ends inside that prefix, and is the key's entity too.
    #[test]
    fn an_entity_holds_for_every_key_that_extends_its_prefix(key in keys()) {
        for cut in 0..=key.len() {
            if let Some(len) = entity_len(&key[..cut]) {
                prop_assert!(len <= cut);
                prop_assert_eq!(entity_len(&key), Some(len));
            }
        }
    }

    /// A key's family is the first in the table whose start it begins with,
    /// whatever shortcut the lookup takes.
    #[test]
    fn a_key_falls_to_the_first_family_its_start_begins(key in keys()) {
        let first = FAMILIES.iter().find(|family| key.starts_with(family.start));
        prop_assert!(std::ptr::eq(
            family_of(&key).map_or(std::ptr::null(), |family| family),
            first.map_or(std::ptr::null(), |family| family),
        ));
    }
}
