//! Patches may not change `@chunk` settings or a generated chunk collection.

use defra_core::DEFAULT_SPLITTER_VERSION;
use schema::definition_validation::{
    validate_collection_changes, validate_new_collections_with_existing,
};
use schema::{
    CType, ChunkSettingsChange, ChunkSource, ChunkedField, CollectionVersion, Commitments,
    FieldDescription, FieldKind, QuerySource,
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
        splitter_version: DEFAULT_SPLITTER_VERSION,
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
        splitter_version: DEFAULT_SPLITTER_VERSION,
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

/// Any version other than the default.
const OTHER_SPLITTER: u32 = DEFAULT_SPLITTER_VERSION + 1;

fn on_splitter(version: u32) -> CollectionVersion {
    let mut article = article();
    article.chunked_fields[0].splitter_version = version;
    article
}

#[test]
fn changing_the_splitter_version_is_refused() {
    let err = check(
        vec![article(), chunk()],
        vec![on_splitter(OTHER_SPLITTER), chunk()],
    )
    .unwrap_err();
    assert!(err.contains("chunk settings cannot be mutated"), "{err}");
    assert_eq!(
        schema::chunk_settings_change(&article(), &on_splitter(OTHER_SPLITTER)),
        Some(ChunkSettingsChange::SplitterVersion)
    );
    let mut wider = on_splitter(OTHER_SPLITTER);
    wider.chunked_fields[0].max_chars = 800;
    assert_eq!(
        schema::chunk_settings_change(&article(), &wider),
        Some(ChunkSettingsChange::ChunkedFields)
    );
}

#[test]
fn the_splitter_version_names_itself_in_the_refusal() {
    let err =
        schema::ensure_chunk_settings_kept(&article(), &on_splitter(OTHER_SPLITTER)).unwrap_err();
    assert!(
        err.to_string()
            .ends_with("Changed: the @chunk splitter version"),
        "{err}"
    );
}

/// A first version on rules this node lacks is still valid; only cutting its text fails.
#[test]
fn a_first_version_on_another_splitter_version_passes_the_first_version_rules() {
    assert!(first_version_errors(&on_splitter(OTHER_SPLITTER)).is_empty());
}

fn identity(version: &CollectionVersion) -> cid::Cid {
    schema::generate_collection_cid_full_with_query(
        Some(&version.name),
        &[],
        1,
        &[],
        None,
        None,
        Commitments::of(version),
    )
    .unwrap()
}

#[test]
fn the_splitter_version_is_part_of_the_identity() {
    assert_eq!(
        identity(&article()),
        identity(&on_splitter(DEFAULT_SPLITTER_VERSION))
    );
    assert_ne!(identity(&article()), identity(&on_splitter(OTHER_SPLITTER)));
}

/// The stored record omits the default, so records stored before it read the same.
#[test]
fn the_stored_record_omits_the_default_splitter_version() {
    let stored = serde_json::to_value(&article().chunked_fields[0]).unwrap();
    assert_eq!(
        stored,
        serde_json::json!({ "Field": "body", "MaxChars": 1600, "Overlap": 0 })
    );
    let read: ChunkedField = serde_json::from_value(stored).unwrap();
    assert_eq!(read.splitter_version, DEFAULT_SPLITTER_VERSION);
    let versioned = serde_json::to_value(&on_splitter(OTHER_SPLITTER).chunked_fields[0]).unwrap();
    assert_eq!(versioned["SplitterVersion"], OTHER_SPLITTER);
    let read: ChunkedField = serde_json::from_value(versioned).unwrap();
    assert_eq!(read, on_splitter(OTHER_SPLITTER).chunked_fields[0]);
}

const RENAMED: &str = "Story";
const RENAMED_VERSION: &str = "v-renamed";
const CHANGED_NAME: &str = "Changed: the collection name";

/// A patch gives a renamed version a new version ID, so only the chunk rule sees the rename.
fn renamed(mut version: CollectionVersion) -> CollectionVersion {
    version.name = RENAMED.into();
    version.version_id = RENAMED_VERSION.into();
    version
}

fn active(mut version: CollectionVersion) -> CollectionVersion {
    version.is_active = true;
    version
}

#[test]
fn renaming_the_parent_is_refused() {
    assert_eq!(
        schema::chunk_settings_change(&article(), &renamed(article())),
        Some(ChunkSettingsChange::Name)
    );
    let err = check(
        vec![active(article()), active(chunk())],
        vec![active(renamed(article())), active(chunk())],
    )
    .unwrap_err();
    assert!(err.contains(CHANGED_NAME), "{err}");
}

#[test]
fn renaming_the_chunk_collection_is_refused() {
    assert_eq!(
        schema::chunk_settings_change(&chunk(), &renamed(chunk())),
        Some(ChunkSettingsChange::Name)
    );
    let err = schema::ensure_chunk_settings_kept(&chunk(), &renamed(chunk())).unwrap_err();
    assert!(err.to_string().ends_with(CHANGED_NAME), "{err}");
}

#[test]
fn renaming_a_collection_without_chunk_settings_is_allowed() {
    let plain = CollectionVersion::new(
        "Author",
        "v-author",
        "c-author",
        vec![FieldDescription::new(
            "1",
            "_docID",
            FieldKind::doc_id(),
            CType::LwwRegister,
        )],
    );
    assert_eq!(
        schema::chunk_settings_change(&plain, &renamed(plain.clone())),
        None
    );
}
