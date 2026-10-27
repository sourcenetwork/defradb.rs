//! Patches may not change `@chunk` settings or a generated chunk collection.

use schema::definition_validation::validate_collection_changes;
use schema::{CType, ChunkSource, ChunkedField, CollectionVersion, FieldDescription, FieldKind};

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
