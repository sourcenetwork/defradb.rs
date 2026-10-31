use super::super::parser::parse_sdl;
use schema::{FieldKind, CHUNK_SOURCE_ID_FIELD};

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk
    }
"#;

#[test]
fn chunk_generates_a_collection_after_its_parent() {
    let collections = parse_sdl(SDL).unwrap();
    let names: Vec<&str> = collections.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Article", "Article__body_chunk"]);
}

#[test]
fn chunk_collection_has_source_text_and_hash() {
    let collections = parse_sdl(SDL).unwrap();
    let (article, chunk) = (&collections[0], &collections[1]);
    let names: Vec<&str> = chunk.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["_docID", "_sourceID", "hash", "source", "text"]);
    assert_eq!(
        chunk.field_by_name("text").unwrap().kind,
        FieldKind::string()
    );
    let source = chunk.field_by_name("source").unwrap();
    assert!(source.is_primary);
    assert_eq!(
        source.kind,
        FieldKind::relation(article.collection_id.clone(), false)
    );
    assert_eq!(chunk.indexes.len(), 1);
    let indexed: Vec<&str> = chunk.indexes[0]
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(indexed, vec![CHUNK_SOURCE_ID_FIELD, "hash"]);
    let source_of = chunk.chunk_of.as_ref().unwrap();
    assert_eq!(source_of.collection, "Article");
    assert_eq!(source_of.field_name, "body");
    assert!(chunk.policy.is_none());
}

#[test]
fn parent_gets_the_list_field_and_settings() {
    let collections = parse_sdl(SDL).unwrap();
    let article = &collections[0];
    assert_eq!(
        article.field_by_name("_body_chunks").unwrap().kind,
        FieldKind::string_array()
    );
    let chunked = article.chunked_field("body").unwrap();
    assert_eq!(chunked.collection(&article.name), "Article__body_chunk");
    assert_eq!((chunked.max_chars, chunked.overlap), (1600, 0));
    assert!(article.chunk_of.is_none());
    assert!(article.field_by_name("Article__body_chunk").is_none());
}

#[test]
fn chunk_takes_max_chars_and_overlap() {
    let collections =
        parse_sdl("type Note { body: String @chunk(maxChars: 300, overlap: 2) }").unwrap();
    let chunked = collections[0].chunked_field("body").unwrap();
    assert_eq!((chunked.max_chars, chunked.overlap), (300, 2));
}

#[test]
fn chunk_ids_are_stable_across_parses() {
    let first = parse_sdl(SDL).unwrap();
    let second = parse_sdl(SDL).unwrap();
    assert_eq!(first, second);
}

#[test]
fn chunk_parent_keeps_its_policy_and_the_chunks_get_none() {
    let collections = parse_sdl(
        r#"type Article @policy(id: "p1", resource: "articles") {
            body: String @chunk
        }"#,
    )
    .unwrap();
    assert!(collections[0].policy.is_some());
    assert!(collections[1].policy.is_none());
    assert!(collections[1].reads_are_gated());
}

#[test]
fn chunk_is_refused_off_a_plain_string_field() {
    for sdl in [
        "type A { n: Int @chunk }",
        "type A { tags: [String] @chunk }",
        "type A { body: String @chunk @crdt(type: pcounter) }",
        "interface A { body: String @chunk }",
    ] {
        let err = parse_sdl(sdl).unwrap_err().to_string();
        assert!(err.contains("@chunk"), "{sdl}: {err}");
    }
}

#[test]
fn chunk_refuses_a_tiny_limit() {
    let err = parse_sdl("type A { body: String @chunk(maxChars: 10) }")
        .unwrap_err()
        .to_string();
    assert!(err.contains("maxChars must be at least 64"), "{err}");
}

#[test]
fn chunk_refuses_names_that_are_taken() {
    let taken_type = r#"
        type A { body: String @chunk }
        type A__body_chunk { x: String }
    "#;
    assert!(parse_sdl(taken_type)
        .unwrap_err()
        .to_string()
        .contains("already taken"));
    let taken_field = "type A { body: String @chunk _body_chunks: [String] }";
    assert!(parse_sdl(taken_field)
        .unwrap_err()
        .to_string()
        .contains("already taken"));
}

#[test]
fn chunk_settings_are_part_of_both_identities() {
    let ids = |max_chars: u32, overlap: u32| {
        let sdl = format!(
            "type Note {{ body: String @chunk(maxChars: {max_chars}, overlap: {overlap}) }}"
        );
        parse_sdl(&sdl)
            .unwrap()
            .into_iter()
            .map(|c| (c.collection_id, c.version_id))
            .collect::<Vec<_>>()
    };
    let base = ids(64, 0);
    assert_ne!(base, ids(1600, 0));
    assert_ne!(base, ids(64, 1));
    for (other, base) in ids(1600, 0).iter().zip(&base) {
        assert_ne!(other.0, base.0);
        assert_ne!(other.1, base.1);
    }
}

#[test]
fn chunk_field_order_does_not_change_the_identity() {
    let first =
        parse_sdl("type Note { a: String @chunk  b: String @chunk(maxChars: 300) }").unwrap();
    let second =
        parse_sdl("type Note { b: String @chunk(maxChars: 300)  a: String @chunk }").unwrap();
    assert_eq!(first[0].version_id, second[0].version_id);
}
