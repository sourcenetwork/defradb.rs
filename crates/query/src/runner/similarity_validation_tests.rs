//! A misused `SIMILARITY` fails validation with Go's text wherever it is
//! selected: directly, in a fragment, under a relation, or in a mutation.

use schema::{CType, CollectionVersion, FieldDescription, FieldKind, ScalarArrayKind};

use crate::executor::{QueryExecutor, QueryRequest};
use crate::runner::QueryRunner;
use crate::test_utils::MockFetcher;

fn collections() -> Vec<CollectionVersion> {
    let users = CollectionVersion::new(
        "User",
        "v1",
        "coll-user",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "name", FieldKind::string(), CType::LwwRegister),
            FieldDescription::new(
                "3",
                "pets",
                FieldKind::ScalarArray(ScalarArrayKind::StringArray),
                CType::LwwRegister,
            ),
            FieldDescription::new(
                "4",
                "points",
                FieldKind::ScalarArray(ScalarArrayKind::Float64Array),
                CType::LwwRegister,
            ),
            FieldDescription::new(
                "5",
                "books",
                FieldKind::Relation {
                    collection_id: "coll-book".into(),
                    is_array: true,
                },
                CType::None,
            ),
        ],
    );
    let books = CollectionVersion::new(
        "Book",
        "v1",
        "coll-book",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new(
                "2",
                "tags",
                FieldKind::ScalarArray(ScalarArrayKind::StringArray),
                CType::LwwRegister,
            ),
        ],
    );
    vec![users, books]
}

fn errors(query: &str) -> Vec<String> {
    workers::block_on(async {
        let runner = QueryRunner::new(MockFetcher::new(), collections());
        let response = runner.execute(QueryRequest::new(query)).await;
        response.errors.into_iter().map(|e| e.message).collect()
    })
    .unwrap()
}

const PETS: &str = "similarity can only target a numeric array field. Field: pets, Type: [String!]";

#[test]
fn similarity_without_argument_names_the_missing_target() {
    assert_eq!(
        errors("query { User { SIMILARITY } }"),
        ["similarity requires a target field argument"]
    );
}

#[test]
fn similarity_on_non_vector_field_fails_like_go() {
    assert_eq!(
        errors("query { User { name SIMILARITY(pets: {vector: [1.1, 1.2]}) } }"),
        [PETS]
    );
    assert_eq!(
        errors("query { User { ... on User { SIMILARITY(pets: {vector: [1.1]}) } } }"),
        [PETS]
    );
}

#[test]
fn similarity_in_a_fragment_under_a_relation_fails_like_go() {
    assert_eq!(
        errors(
            "query { User { books { ...similarity } } } \
             fragment similarity on Book { SIMILARITY(tags: {vector: [1.1]}) }"
        ),
        ["similarity can only target a numeric array field. Field: tags, Type: [String!]"]
    );
}

#[test]
fn similarity_in_a_mutation_selection_fails_like_go() {
    assert_eq!(
        errors(
            r#"mutation { add_User(input: {name: "John"}) { SIMILARITY(pets: {vector: [1.1, 1.2, 0.9]}) } }"#
        ),
        [PETS]
    );
}

#[test]
fn similarity_on_unknown_field_keeps_unknown_argument_error() {
    let errors = errors("query { User { SIMILARITY(nope: {vector: [1.1]}) } }");
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].contains("Unknown argument \"nope\""),
        "{errors:?}"
    );
}

#[test]
fn similarity_on_vector_field_passes_validation() {
    assert!(errors("query { User { SIMILARITY(points: {vector: [1.1]}) } }").is_empty());
}
