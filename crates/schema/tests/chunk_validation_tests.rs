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

fn generated_chunk() -> CollectionVersion {
    let mut chunk = chunk();
    for (id, name) in [("3", "_sourceID"), ("4", "hash"), ("5", "source")] {
        chunk.fields.push(FieldDescription::new(
            id,
            name,
            FieldKind::string(),
            CType::LwwRegister,
        ));
    }
    chunk
}

fn first_version_errors(version: &CollectionVersion) -> Vec<String> {
    schema::chunk_settings_errors(version)
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_first_version_the_sdl_could_build_passes() {
    assert!(first_version_errors(&article()).is_empty());
    assert!(first_version_errors(&generated_chunk()).is_empty());
    assert!(schema::definition_validation::validate_new_collections(&[
        article(),
        generated_chunk()
    ])
    .is_ok());
}

#[test]
fn a_first_version_below_the_minimum_max_chars_is_refused() {
    let mut article = article();
    article.chunked_fields[0].max_chars = schema::MIN_CHUNK_MAX_CHARS - 1;
    let err = schema::definition_validation::validate_new_collections(&[article]).unwrap_err();
    assert!(
        err.contains("maxChars must be at least 64, got 63"),
        "{err}"
    );
}

#[test]
fn a_chunk_parent_on_a_collection_with_another_name_is_refused() {
    let mut article = article();
    article.chunk_of = Some(ChunkSource {
        collection: "Article".into(),
        field_name: "body".into(),
    });
    let errors = first_version_errors(&article);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("whose chunk collection is Article__body_chunk")),
        "{errors:?}"
    );
}

#[test]
fn a_chunked_field_without_its_list_field_is_refused() {
    let mut article = article();
    article.fields.retain(|field| field.name != "_body_chunks");
    let errors = first_version_errors(&article);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("needs the list field _body_chunks")),
        "{errors:?}"
    );
}

#[test]
fn reordered_chunk_fields_pass() {
    let summary = ChunkedField {
        field_name: "summary".into(),
        max_chars: 800,
        overlap: 1,
    };
    let with_summary = || {
        let mut article = article();
        article.fields.push(FieldDescription::new(
            "4",
            "_summary_chunks",
            FieldKind::string_array(),
            CType::LwwRegister,
        ));
        article.fields.push(FieldDescription::new(
            "5",
            "summary",
            FieldKind::string(),
            CType::LwwRegister,
        ));
        article
    };
    let mut stored = with_summary();
    stored.chunked_fields.insert(0, summary.clone());
    let mut patched = with_summary();
    patched.chunked_fields.push(summary);
    let result = check(vec![stored, chunk()], vec![patched, chunk()]);
    assert!(result.is_ok(), "{result:?}");
}
