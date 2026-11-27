//! Reading a chunk's hash from its `[_sourceID, hash]` index entry.

use document::NormalValue;

use super::entry_hash;

fn entry(hash: NormalValue) -> Vec<NormalValue> {
    vec![NormalValue::String("parent".into()), hash]
}

#[test]
fn a_string_entry_gives_its_hash() {
    assert_eq!(
        entry_hash(&entry(NormalValue::String("h1".into()))),
        Some("h1")
    );
}

#[test]
fn a_bytes_entry_gives_its_hash_as_text() {
    assert_eq!(
        entry_hash(&entry(NormalValue::Bytes(b"h1".to_vec()))),
        Some("h1")
    );
}

#[test]
fn bytes_that_are_not_text_give_no_hash() {
    assert_eq!(
        entry_hash(&entry(NormalValue::Bytes(vec![0xff, 0xfe]))),
        None
    );
}

#[test]
fn an_entry_without_a_hash_gives_none() {
    assert_eq!(entry_hash(&[NormalValue::String("parent".into())]), None);
}

#[test]
fn a_hash_of_another_kind_gives_none() {
    assert_eq!(entry_hash(&entry(NormalValue::Int(7))), None);
}
