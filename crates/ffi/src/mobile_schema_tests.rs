use super::plan_bootstrap;
use schema::CollectionVersion;

const CHUNKED: &str = "type Article { body: String @chunk }";
const PLAIN: &str = "type Article { body: String }";
const TWO_CHUNKED: &str =
    "type Article { body: String @chunk(maxChars: 64) abstract: String @chunk(maxChars: 80) }";
const CHANGED_FIELDS: &str = "Changed: the @chunk fields";

fn parse(sdl: &str) -> Vec<CollectionVersion> {
    query::parse_sdl(sdl).unwrap()
}

/// `sdl` as a peer's definition leaves it before activation.
fn synced_inactive(sdl: &str) -> Vec<CollectionVersion> {
    let mut versions = parse(sdl);
    for version in &mut versions {
        version.is_active = false;
    }
    versions
}

fn names(collections: &[CollectionVersion]) -> Vec<&str> {
    collections.iter().map(|c| c.name.as_str()).collect()
}

#[test]
fn a_new_chunked_type_creates_its_chunk_collection_with_it() {
    let plan = plan_bootstrap(parse(CHUNKED), &[]).unwrap();
    assert_eq!(names(&plan.create), vec!["Article", "Article__body_chunk"]);
    assert!(plan.skipped.is_empty());
}

#[test]
fn a_stored_chunked_type_skips_its_chunk_collection_with_it() {
    let plan = plan_bootstrap(parse(CHUNKED), &parse(CHUNKED)).unwrap();
    assert!(plan.create.is_empty());
    assert_eq!(plan.skipped, vec!["Article", "Article__body_chunk"]);
}

#[test]
fn adding_chunk_to_a_stored_plain_type_is_refused() {
    let err = plan_bootstrap(parse(CHUNKED), &parse(PLAIN)).err().unwrap();
    assert!(err.contains("Collection: Article,"), "{err}");
    assert!(err.contains(CHANGED_FIELDS), "{err}");
}

#[test]
fn removing_chunk_from_a_stored_chunked_type_is_refused() {
    assert!(plan_bootstrap(parse(PLAIN), &parse(CHUNKED)).is_err());
}

#[test]
fn a_synced_inactive_collection_is_skipped() {
    let plan = plan_bootstrap(parse(CHUNKED), &synced_inactive(CHUNKED)).unwrap();
    assert!(plan.create.is_empty());
    assert_eq!(plan.skipped, vec!["Article", "Article__body_chunk"]);
}

#[test]
fn a_synced_inactive_parent_skips_its_chunk_collection() {
    let mut stored = synced_inactive(CHUNKED);
    stored.retain(|version| version.chunk_of.is_none());
    let plan = plan_bootstrap(parse(CHUNKED), &stored).unwrap();
    assert!(plan.create.is_empty());
    assert_eq!(plan.skipped, vec!["Article", "Article__body_chunk"]);
}

#[test]
fn a_stored_version_with_unsorted_chunk_fields_matches_the_sdl() {
    let mut stored = synced_inactive(TWO_CHUNKED);
    stored[0].chunked_fields.reverse();
    assert_ne!(
        stored[0].chunked_fields,
        parse(TWO_CHUNKED)[0].chunked_fields
    );
    let plan = plan_bootstrap(parse(TWO_CHUNKED), &stored).unwrap();
    assert!(plan.create.is_empty());
    assert_eq!(plan.skipped.len(), 3);
}

#[test]
fn other_chunk_settings_than_a_synced_inactive_version_are_refused() {
    let changed = "type Article { body: String @chunk(maxChars: 128) }";
    let err = plan_bootstrap(parse(changed), &synced_inactive(CHUNKED))
        .err()
        .unwrap();
    assert!(err.contains(CHANGED_FIELDS), "{err}");
}

/// A peer on newer splitting rules: this node's SDL would cut its text differently.
#[test]
fn another_splitter_version_than_a_synced_inactive_version_is_refused() {
    let mut stored = synced_inactive(CHUNKED);
    stored[0].chunked_fields[0].splitter_version = defra_core::DEFAULT_SPLITTER_VERSION + 1;
    let err = plan_bootstrap(parse(CHUNKED), &stored).err().unwrap();
    assert!(err.contains("Collection: Article,"), "{err}");
    assert!(
        err.contains("Changed: the @chunk splitter version"),
        "{err}"
    );
}

#[test]
fn a_placeholder_counts_as_existing_without_a_settings_check() {
    let mut stored = synced_inactive(PLAIN);
    stored[0].is_placeholder = true;
    let plan = plan_bootstrap(parse(CHUNKED), &stored).unwrap();
    assert!(plan.create.is_empty());
    assert_eq!(plan.skipped, vec!["Article", "Article__body_chunk"]);
}

#[test]
fn the_first_version_decides_over_a_later_one() {
    let mut stored = synced_inactive(CHUNKED);
    let mut legacy = parse(PLAIN).remove(0);
    legacy.version_id = "later".to_string();
    legacy.previous_version = Some(schema::CollectionSource::new(&stored[0].version_id));
    legacy.is_active = true;
    stored.insert(0, legacy);
    assert!(plan_bootstrap(parse(CHUNKED), &stored).is_ok());
    assert!(plan_bootstrap(parse(PLAIN), &stored).is_err());
}
