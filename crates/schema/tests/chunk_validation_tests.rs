//! Patches may not change `@chunk` settings or a generated chunk collection.

use schema::definition_validation::{
    validate_collection_changes, validate_new_collections_with_existing,
};
use schema::{
    CType, ChunkSource, ChunkedField, CollectionVersion, FieldDescription, FieldKind, QuerySource,
};

fn article() -> CollectionVersion {
    let mut article = CollectionVersion::new(
        "Article",
        "v-article",
        "c-article",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new(
                "2",
                "_body_chunks",
                FieldKind::string_array(),
                CType::LwwRegister,
            ),
            FieldDescription::new("3", "body", FieldKind::string(), CType::LwwRegister),
        ],
    );
    article.chunked_fields.push(ChunkedField {
        field_name: "body".into(),
        max_chars: 1600,
        overlap: 0,
    });
    article
}

fn chunk() -> CollectionVersion {
    let mut chunk = CollectionVersion::new(
        "Article__body_chunk",
        "v-chunk",
        "c-chunk",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "text", FieldKind::string(), CType::LwwRegister),
        ],
    );
    chunk.chunk_of = Some(ChunkSource {
        collection: "Article".into(),
        field_name: "body".into(),
    });
    chunk
}

fn check(old: Vec<CollectionVersion>, new: Vec<CollectionVersion>) -> Result<(), String> {
    validate_collection_changes(&old, &new)
}

#[test]
fn unchanged_chunk_settings_pass() {
    assert!(check(vec![article(), chunk()], vec![article(), chunk()]).is_ok());
}

#[test]
fn changing_max_chars_is_refused() {
    let mut patched = article();
    patched.chunked_fields[0].max_chars = 800;
    let err = check(vec![article(), chunk()], vec![patched, chunk()]).unwrap_err();
    assert!(err.contains("chunk settings cannot be mutated"), "{err}");
}

#[test]
fn dropping_the_generated_flag_is_refused() {
    let mut patched = chunk();
    patched.chunk_of = None;
    let err = check(vec![article(), chunk()], vec![article(), patched]).unwrap_err();
    assert!(err.contains("chunk settings cannot be mutated"), "{err}");
}

#[test]
fn adding_a_field_to_a_chunk_collection_is_refused() {
    let mut patched = chunk();
    patched.fields.push(FieldDescription::new(
        "3",
        "extra",
        FieldKind::string(),
        CType::LwwRegister,
    ));
    let err = check(vec![article(), chunk()], vec![article(), patched]).unwrap_err();
    assert!(err.contains("fields cannot be mutated"), "{err}");
}

fn placeholder(of: &CollectionVersion) -> CollectionVersion {
    let mut placeholder =
        CollectionVersion::new(&of.name, &of.version_id, &of.collection_id, vec![]);
    placeholder.is_placeholder = true;
    placeholder
}

#[test]
fn filling_a_placeholder_with_chunk_settings_is_refused() {
    let err = check(vec![placeholder(&article())], vec![article()]).unwrap_err();
    assert!(
        err.contains("chunk settings cannot be added by a patch"),
        "{err}"
    );
}

#[test]
fn filling_a_placeholder_with_a_chunk_collection_is_refused() {
    let err = check(vec![placeholder(&chunk())], vec![chunk()]).unwrap_err();
    assert!(
        err.contains("chunk settings cannot be added by a patch"),
        "{err}"
    );
}

#[test]
fn a_view_with_a_chunked_field_is_refused() {
    let mut view = article();
    view.query = Some(QuerySource::new(serde_json::json!({ "Name": "Source" })));
    let err = validate_new_collections_with_existing(&[view, chunk()], &[]).unwrap_err();
    assert_eq!(err, "@chunk is not allowed in a view. Field: Article.body");
}

#[test]
fn a_chunked_collection_that_is_not_a_view_is_created() {
    assert!(validate_new_collections_with_existing(&[article(), chunk()], &[]).is_ok());
}

fn embedding(field: &str, source: &str, model: &str) -> schema::VectorEmbeddingDescription {
    schema::VectorEmbeddingDescription::new(field, model, "openai").with_fields(vec![source.into()])
}

#[test]
fn changing_a_chunk_collections_embedding_is_refused() {
    let mut embedded = chunk();
    embedded
        .vector_embeddings
        .push(embedding("body_v", "text", "m1"));
    for patched in [vec![], vec![embedding("body_v", "text", "m2")]] {
        let mut changed = embedded.clone();
        changed.vector_embeddings = patched;
        let err = check(vec![article(), embedded.clone()], vec![article(), changed]).unwrap_err();
        assert!(
            err.contains("a generated chunk collection's embeddings cannot be mutated. Collection: Article__body_chunk"),
            "{err}"
        );
    }
}

#[test]
fn an_embedding_of_a_chunked_field_on_the_parent_is_refused() {
    let mut patched = article();
    patched
        .vector_embeddings
        .push(embedding("body_v", "body", "m1"));
    let err = check(vec![article(), chunk()], vec![patched, chunk()]).unwrap_err();
    assert!(
        err.contains("an embedding over a chunked field belongs to its chunk collection. Field: Article.body_v"),
        "{err}"
    );
}
