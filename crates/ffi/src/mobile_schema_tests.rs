use super::plan_bootstrap;
use schema::CollectionVersion;

const CHUNKED: &str = "type Article { body: String @chunk }";
const PLAIN: &str = "type Article { body: String }";

fn parse(sdl: &str) -> Vec<CollectionVersion> {
    query::parse_sdl(sdl).unwrap()
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
    assert_eq!(
        err,
        "the @chunk fields of Article differ from the stored collection, and they cannot be added, removed or changed after it is created. Collection: Article"
    );
}

#[test]
fn removing_chunk_from_a_stored_chunked_type_is_refused() {
    assert!(plan_bootstrap(parse(PLAIN), &parse(CHUNKED)).is_err());
}
