//! Creating an index the collection already has is refused, where Go adds a
//! second, `_2`-suffixed copy (#3320). The refusals are listed in
//! `.github/testo/divergences.txt`.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{Actions, AddCollection, ListIndexes, NewIndex};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn users(sdl_name: &str) -> Actions {
    AddCollection {
        sdl: format!(
            r#"
                type User {{
                    name: String {sdl_name}
                    age: Int
                }}
            "#
        )
        .into(),
        ..Default::default()
    }
    .into()
}

fn new_index(
    collection: &str,
    name: &str,
    unique: bool,
    fields: &[(&str, bool)],
    error: Option<&str>,
) -> Actions {
    NewIndex {
        collection: collection.into(),
        index: IndexSpec {
            name: name.into(),
            unique,
            fields: fields
                .iter()
                .map(|(name, descending)| IndexField {
                    name: (*name).into(),
                    descending: *descending,
                })
                .collect(),
            ..Default::default()
        },
        expected_error: error.map(Into::into),
        ..Default::default()
    }
    .into()
}

fn listed(collection: &str, names: &[&str]) -> Actions {
    let names: Vec<Value> = names
        .iter()
        .map(|name| m::object(json!({ "Description": m::object(json!({ "Name": name })) })))
        .collect();
    ListIndexes {
        collection: collection.into(),
        expected_data: json!({ "indexes": m::unordered(Value::Array(names)) }),
        ..Default::default()
    }
    .into()
}

fn run(id: &'static str, actions: Vec<Actions>) {
    TestCase {
        id,
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.definition.create-duplicate")]
fn index_new_same_generated_index_twice_is_refused() {
    run(
        integration_test::scenario_id!(),
        vec![
            users(""),
            new_index("User", "", false, &[("name", false)], None),
            new_index(
                "User",
                "",
                false,
                &[("name", false)],
                Some("index with the same definition already exists. Name: User_name_ASC"),
            ),
            listed("User", &["User_name_ASC"]),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.definition.create-duplicate")]
fn index_new_named_copy_of_existing_index_is_refused() {
    run(
        integration_test::scenario_id!(),
        vec![
            users(""),
            new_index("User", "by_name", false, &[("name", false)], None),
            new_index(
                "User",
                "other_name",
                false,
                &[("name", false)],
                Some("index with the same definition already exists. Name: by_name"),
            ),
            new_index(
                "User",
                "",
                false,
                &[("name", false)],
                Some("index with the same definition already exists. Name: by_name"),
            ),
            listed("User", &["by_name"]),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.definition.create-duplicate")]
fn index_new_copy_of_sdl_index_is_refused() {
    run(
        integration_test::scenario_id!(),
        vec![
            users("@index"),
            new_index(
                "User",
                "",
                false,
                &[("name", false)],
                Some("index with the same definition already exists. Name: User_name_ASC"),
            ),
            listed("User", &["User_name_ASC"]),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.definition.create-duplicate")]
fn index_new_same_unique_index_twice_is_refused() {
    run(
        integration_test::scenario_id!(),
        vec![
            users(""),
            new_index("User", "by_name", true, &[("name", false)], None),
            new_index(
                "User",
                "",
                true,
                &[("name", false)],
                Some("index with the same definition already exists. Name: by_name"),
            ),
            listed("User", &["by_name"]),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.definition.create-duplicate")]
fn index_new_taken_name_reports_the_name() {
    run(
        integration_test::scenario_id!(),
        vec![
            users(""),
            new_index("User", "by_name", false, &[("name", false)], None),
            new_index(
                "User",
                "by_name",
                false,
                &[("name", false)],
                Some("index with name already exists. Name: by_name"),
            ),
            listed("User", &["by_name"]),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.definition.create-duplicate")]
fn index_new_differing_definitions_are_created() {
    run(
        integration_test::scenario_id!(),
        vec![
            AddCollection {
                sdl: r#"
                    type User @index(name: "by_name_desc", includes: [{field: "name", direction: DESC}]) {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            new_index("User", "by_name", false, &[("name", false)], None),
            new_index("User", "by_name_unique", true, &[("name", false)], None),
            new_index(
                "User",
                "by_name_age",
                false,
                &[("name", false), ("age", false)],
                None,
            ),
            new_index(
                "User",
                "by_age_name",
                false,
                &[("age", false), ("name", false)],
                None,
            ),
            listed(
                "User",
                &[
                    "by_name_desc",
                    "by_name",
                    "by_name_unique",
                    "by_name_age",
                    "by_age_name",
                ],
            ),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.definition.create-duplicate")]
fn index_new_over_the_hidden_relation_index_is_created() {
    run(
        integration_test::scenario_id!(),
        vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }
                    type Author {
                        name: String
                        books: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            new_index("Book", "by_author", false, &[("_authorID", false)], None),
            listed("Book", &["by_author"]),
        ],
    );
}

/// A vector index the collection already has is refused too, whichever of its
/// unset defaults the second request spells out. A vector index repeated with
/// different parameters is a different index and is created.
#[test]
#[mantra_macros::req_test("defra.index.definition.create-duplicate")]
fn index_new_same_vector_index_twice_is_refused() {
    let vector_sdl = r#"
        type User {
            name: String
            embedding: [Float32!]
        }
    "#;
    let by_embedding = |name: &str, config: Value, error: Option<&str>| -> Actions {
        NewIndex {
            collection: "User".into(),
            index: IndexSpec {
                name: name.into(),
                fields: vec![IndexField {
                    name: "embedding".into(),
                    descending: false,
                }],
                vector: Some(config),
                ..Default::default()
            },
            expected_error: error.map(Into::into),
            ..Default::default()
        }
        .into()
    };
    run(
        integration_test::scenario_id!(),
        vec![
            AddCollection {
                sdl: vector_sdl.into(),
                ..Default::default()
            }
            .into(),
            // The stored definition carries the defaults filled in.
            by_embedding(
                "by_embedding",
                json!({ "Dimensions": 3, "HNSW": {}, "Metric": "COSINE" }),
                None,
            ),
            // The same index with every HNSW default spelled out as unset is
            // the same definition once normalized, so it is refused.
            by_embedding(
                "",
                json!({
                    "Dimensions": 3,
                    "HNSW": { "M": 0, "EfConstruction": 0, "EfSearch": 0 },
                    "Metric": "COSINE",
                }),
                Some("index with the same definition already exists. Name: by_embedding"),
            ),
            // A different HNSW parameter is a different index.
            by_embedding(
                "by_embedding_wide",
                json!({ "Dimensions": 3, "HNSW": { "M": 32 }, "Metric": "COSINE" }),
                None,
            ),
            listed("User", &["by_embedding", "by_embedding_wide"]),
        ],
    );
}
