//! A parent finds the embedding of its `@chunk` field on its chunk collection.

use schema::{
    CType, ChunkSource, ChunkedField, CollectionVersion, FieldDescription, FieldKind,
    VectorEmbeddingDescription,
};

fn field(id: &str, name: &str, kind: FieldKind) -> FieldDescription {
    FieldDescription::new(id, name, kind, CType::LwwRegister)
}

fn article(extra: Vec<FieldDescription>) -> CollectionVersion {
    let mut fields = vec![field("1", "body", FieldKind::string())];
    fields.extend(extra);
    let mut article = CollectionVersion::new("Article", "v-article", "c-article", fields);
    article.chunked_fields = vec![ChunkedField {
        field_name: "body".into(),
        max_chars: 64,
        overlap: 0,
    }];
    article
}

/// A chunk collection of `parent`'s `field` that embeds its text into `body_v`.
fn chunks_of(parent: &str, field_name: &str) -> CollectionVersion {
    let mut chunks = CollectionVersion::new(
        format!("{parent}__{field_name}_chunk"),
        "v-chunks",
        "c-chunks",
        vec![field("1", "body_v", FieldKind::float32_array())],
    );
    chunks.chunk_of = Some(ChunkSource {
        collection: parent.into(),
        field_name: field_name.into(),
    });
    chunks.vector_embeddings = vec![VectorEmbeddingDescription {
        field_name: "body_v".into(),
        fields: vec!["text".into()],
        model: "m".into(),
        provider: "openai".into(),
        template: String::new(),
        url: String::new(),
    }];
    chunks
}

#[test]
fn a_parent_names_the_chunk_collection_of_each_chunked_field() {
    let names: Vec<String> = article(Vec::new()).chunk_collection_names().collect();
    assert_eq!(names, vec!["Article__body_chunk".to_string()]);
}

#[test]
fn the_parent_finds_its_chunk_embedding() {
    let chunks = chunks_of("Article", "body");
    let found = article(Vec::new()).chunk_embedding("body_v", [&chunks]);
    assert_eq!(
        found.map(|chunks| chunks.name.as_str()),
        Some("Article__body_chunk")
    );
}

#[test]
fn a_field_of_the_parent_wins_over_a_chunk_embedding() {
    let chunks = chunks_of("Article", "body");
    let parent = article(vec![field("2", "body_v", FieldKind::float32_array())]);
    assert!(parent.chunk_embedding("body_v", [&chunks]).is_none());
}

#[test]
fn chunks_of_another_parent_or_field_hold_no_embedding_for_it() {
    let parent = article(Vec::new());
    let other_parent = chunks_of("Book", "body");
    let other_field = chunks_of("Article", "summary");
    assert!(parent
        .chunk_embedding("body_v", [&other_parent, &other_field])
        .is_none());
}

#[test]
fn a_chunk_field_without_an_embedding_is_not_a_target() {
    let chunks = chunks_of("Article", "body");
    assert!(article(Vec::new())
        .chunk_embedding("text", [&chunks])
        .is_none());
}
