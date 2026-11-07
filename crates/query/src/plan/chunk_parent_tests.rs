use chunk_split::list::text_hash as hash;
use document::{Document, NormalValue};
use schema::{
    CType, ChunkedField, CollectionBuilder, CollectionVersion, FieldDescription, FieldKind,
    ScalarKind,
};

use super::parent_lists;

fn chunked() -> ChunkedField {
    ChunkedField {
        field_name: "body".into(),
        max_chars: 64,
        overlap: 0,
    }
}

// One collection with every field the check reads, typed loosely.
fn open() -> CollectionVersion {
    ["_sourceID", "text", "hash", "body", "_body_chunks"]
        .iter()
        .enumerate()
        .fold(
            CollectionBuilder::new("Test", "bafytest"),
            |builder, (i, name)| {
                builder.field(FieldDescription::new(
                    i.to_string(),
                    *name,
                    FieldKind::Scalar(ScalarKind::None),
                    CType::LwwRegister,
                ))
            },
        )
        .build()
}

fn chunk(text: &str, claimed: &str) -> Document {
    let mut doc = Document::new(open());
    doc.set("_sourceID", NormalValue::String("bae-parent".into()))
        .unwrap();
    doc.set("text", NormalValue::String(text.into())).unwrap();
    doc.set("hash", NormalValue::String(claimed.into()))
        .unwrap();
    doc
}

fn parent(text: &str, list: Vec<String>) -> Document {
    let mut doc = Document::new(open());
    doc.set("body", NormalValue::String(text.into())).unwrap();
    doc.set("_body_chunks", NormalValue::StringArray(list))
        .unwrap();
    doc
}

#[test]
fn a_listed_chunk_is_listed() {
    let text = "Some text.";
    let parent = parent("whole", vec![hash("whole"), hash(text)]);
    assert!(parent_lists(&parent, &chunk(text, &hash(text)), &chunked()));
}

#[test]
fn the_whole_text_hash_does_not_list_a_chunk() {
    let text = "whole";
    let parent = parent(text, vec![hash(text)]);
    assert!(!parent_lists(
        &parent,
        &chunk(text, &hash(text)),
        &chunked()
    ));
}

#[test]
fn a_chunk_claiming_a_listed_hash_for_other_text_is_not_listed() {
    let parent = parent("whole", vec![hash("whole"), hash("real")]);
    assert!(!parent_lists(
        &parent,
        &chunk("forged", &hash("real")),
        &chunked()
    ));
}

#[test]
fn a_deleted_parent_lists_nothing() {
    let text = "Some text.";
    let mut parent = parent("whole", vec![hash("whole"), hash(text)]);
    parent.set_deleted(true);
    assert!(!parent_lists(
        &parent,
        &chunk(text, &hash(text)),
        &chunked()
    ));
}

#[test]
fn a_list_of_other_text_is_ignored_for_the_chunks_of_the_text() {
    let mine = "Mine is short.";
    let theirs = "Theirs is short.";
    let parent = parent(mine, vec![hash(theirs), hash(theirs)]);
    assert!(parent_lists(&parent, &chunk(mine, &hash(mine)), &chunked()));
    assert!(!parent_lists(
        &parent,
        &chunk(theirs, &hash(theirs)),
        &chunked()
    ));
}

#[test]
fn a_parent_without_text_lists_nothing() {
    let text = "Some text.";
    let mut parent = parent("whole", vec![hash("whole"), hash(text)]);
    parent.set("body", NormalValue::Null).unwrap();
    assert!(!parent_lists(
        &parent,
        &chunk(text, &hash(text)),
        &chunked()
    ));
}
