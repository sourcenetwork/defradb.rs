//! Port of Go `tests/integration/collection_version/simple_test.go`.

use integration_test::actions::{AddCollection, IntrospectionRequest};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.define.basic_types")]
fn adds_collection_given_empty_type() {
    TestCase {
        actions: vec![
            AddCollection {
                sdl: "type Users {}",
                ..Default::default()
            }
            .into(),
            IntrospectionRequest {
                request: r#"query { __type (name: "Users") { name } }"#,
                expected_data: json!({ "__type": { "name": "Users" } }),
                ..Default::default()
            }
            .into(),
        ],
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.basic_types")]
fn adds_collection_given_new_types() {
    TestCase {
        actions: vec![
            AddCollection {
                sdl: "type Users {}",
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Books {}",
                ..Default::default()
            }
            .into(),
            IntrospectionRequest {
                request: r#"query { __type (name: "Books") { name } }"#,
                expected_data: json!({ "__type": { "name": "Books" } }),
                ..Default::default()
            }
            .into(),
        ],
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.errors")]
fn errors_given_duplicate_collection() {
    TestCase {
        actions: vec![
            AddCollection {
                sdl: "type Users {}",
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users {}",
                expected_error: Some("collection already exists"),
            }
            .into(),
        ],
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.errors")]
fn errors_given_duplicate_collection_in_same_sdl() {
    TestCase {
        actions: vec![AddCollection {
            sdl: "
                type Users {}
                type Users {}
            ",
            expected_error: Some("collection already exists"),
        }
        .into()],
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.errors")]
fn errors_given_duplicate_collection_in_same_sdl_multiple() {
    TestCase {
        actions: vec![AddCollection {
            sdl: "
                type Users {}
                type Users {}
                type Users {}
            ",
            expected_error: Some(
                "collection already exists. Name: Users\ncollection already exists. Name: Users",
            ),
        }
        .into()],
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.errors")]
fn errors_given_type_with_invalid_field_type() {
    TestCase {
        actions: vec![AddCollection {
            sdl: "
                type Users {
                    name: NotAType
                }
            ",
            expected_error: Some("no type found for given name. Field: name, Kind: NotAType"),
        }
        .into()],
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.errors")]
fn errors_given_type_with_invalid_field_type_multiple() {
    TestCase {
        actions: vec![AddCollection {
            sdl: "
                type Users {
                    name: NotAType
                    age: NotAType
                }
            ",
            expected_error: Some(
                "no type found for given name. Field: age, Kind: NotAType\nno type found for given name. Field: name, Kind: NotAType",
            ),
        }
        .into()],
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.errors")]
fn errors_given_non_null_many_relation_field() {
    TestCase {
        actions: vec![AddCollection {
            sdl: "
                type Dogs {
                    name: String
                    user: Users
                }
                type Users {
                    Dogs: [Dogs!]
                }
            ",
            expected_error: Some("NonNull variants for type are not supported. Type: Dogs"),
        }
        .into()],
    }
    .execute();
}
