use db::chunk::guard::{check_chunk_merge, ChunkMergeError};
use db::chunk::hash::text_hash;
use document::{Document, NormalValue};

fn chunks() -> schema::CollectionVersion {
    query::parse_sdl("type Article { body: String @chunk }")
        .unwrap()
        .into_iter()
        .find(|c| c.name == "Article__body_chunk")
        .expect("chunk collection")
}

fn chunk(parent: &str, text: &str, hash: &str) -> Document {
    let mut doc = Document::new(chunks());
    doc.set("_sourceID", NormalValue::String(parent.into()))
        .unwrap();
    doc.set("text", NormalValue::String(text.into())).unwrap();
    doc.set("hash", NormalValue::String(hash.into())).unwrap();
    doc
}

#[test]
fn a_chunk_whose_hash_is_its_text_is_accepted() {
    let text = "Some text.";
    assert!(check_chunk_merge(None, &chunk("bae-p", text, &text_hash(text)), false).is_ok());
}

#[test]
fn a_chunk_claiming_another_texts_hash_is_refused() {
    let planted = chunk("bae-p", "garbage", &text_hash("Some text."));
    assert!(matches!(
        check_chunk_merge(None, &planted, false),
        Err(ChunkMergeError::HashMismatch)
    ));
}

#[test]
fn a_chunk_cannot_change_after_its_create() {
    let text = "Some text.";
    let old = chunk("bae-p", text, &text_hash(text));
    let changes = [
        chunk("bae-p", "Other.", &text_hash("Other.")),
        chunk("bae-q", text, &text_hash(text)),
        chunk("bae-p", "Other.", &text_hash(text)),
        chunk("bae-p", text, &text_hash("Other.")),
    ];
    for changed in changes {
        assert!(matches!(
            check_chunk_merge(Some(&old), &changed, false),
            Err(ChunkMergeError::Changed(_))
        ));
    }
    assert!(check_chunk_merge(Some(&old), &old.clone(), false).is_ok());
}

#[test]
fn a_merge_cannot_fill_in_a_fixed_field_the_stored_chunk_lacks() {
    let text = "Some text.";
    let mut old = Document::new(chunks());
    old.set("text", NormalValue::String(text.into())).unwrap();
    old.set("hash", NormalValue::String(text_hash(text)))
        .unwrap();
    assert!(matches!(
        check_chunk_merge(Some(&old), &chunk("bae-p", text, &text_hash(text)), false),
        Err(ChunkMergeError::Changed(field)) if field == "_sourceID"
    ));
}

#[test]
fn an_encrypted_chunk_this_node_cannot_read_is_not_hash_checked_here() {
    let mut doc = Document::new(chunks());
    doc.set("_sourceID", NormalValue::String("bae-p".into()))
        .unwrap();
    assert!(check_chunk_merge(None, &doc, true).is_ok());
    assert!(matches!(
        check_chunk_merge(None, &doc, false),
        Err(ChunkMergeError::NotText)
    ));
}

#[test]
fn a_chunk_whose_text_or_hash_is_not_a_string_is_refused() {
    let text = "Some text.";
    let values = [
        (
            Some(NormalValue::Null),
            Some(NormalValue::String(text_hash(text))),
        ),
        (
            Some(NormalValue::String(text.into())),
            Some(NormalValue::Null),
        ),
        (
            Some(NormalValue::Bytes(vec![1, 2, 3])),
            Some(NormalValue::Bytes(vec![4, 5, 6])),
        ),
        (Some(NormalValue::String(text.into())), None),
        (None, Some(NormalValue::String(text_hash(text)))),
    ];
    for (text_value, hash_value) in values {
        let mut doc = Document::new(chunks());
        doc.set("_sourceID", NormalValue::String("bae-p".into()))
            .unwrap();
        if let Some(value) = text_value {
            doc.set_unchecked("text".into(), value);
        }
        if let Some(value) = hash_value {
            doc.set_unchecked("hash".into(), value);
        }
        for encrypted in [false, true] {
            assert!(matches!(
                check_chunk_merge(None, &doc, encrypted),
                Err(ChunkMergeError::NotText)
            ));
        }
    }
}
