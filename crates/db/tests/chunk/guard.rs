use db::chunk::guard::check_chunk_merge;
use db::chunk::list::text_hash;
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
    assert!(check_chunk_merge(None, &chunk("bae-p", text, &text_hash(text))).is_ok());
}

#[test]
fn a_chunk_claiming_another_texts_hash_is_refused() {
    let planted = chunk("bae-p", "garbage", &text_hash("Some text."));
    assert!(check_chunk_merge(None, &planted).is_err());
}

#[test]
fn a_chunk_cannot_change_after_its_create() {
    let text = "Some text.";
    let old = chunk("bae-p", text, &text_hash(text));
    let changed = chunk("bae-p", "Other.", &text_hash("Other."));
    assert!(check_chunk_merge(Some(&old), &changed).is_err());
    assert!(check_chunk_merge(Some(&old), &old.clone()).is_ok());
}

#[test]
fn an_encrypted_chunk_is_not_hash_checked_here() {
    let mut doc = Document::new(chunks());
    doc.set("_sourceID", NormalValue::String("bae-p".into()))
        .unwrap();
    doc.set_unchecked("text".into(), NormalValue::Bytes(vec![1, 2, 3]));
    doc.set_unchecked("hash".into(), NormalValue::Bytes(vec![4, 5, 6]));
    assert!(check_chunk_merge(None, &doc).is_ok());
}
