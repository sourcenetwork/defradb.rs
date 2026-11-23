//! Explain shows a read through an index as Go's plain scan node, with its
//! filter and prefixes.

use integration_test::actions::{AddCollection, GetCollections, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

const PERSON_BOOKS: &str = r#"
    type Person {
        name: String @index
        authoredBooks: [Book]
    }
    type Book {
        title: String
        pages: Int
        author: Person
    }
"#;

/// Captures the version ID of `collection` as `name`.
fn capture_version(collection: &str, name: &str) -> GetCollections {
    GetCollections {
        collection_name: Some(collection.into()),
        filtered: true,
        common: Common {
            capture: captures([(name, "/collections/0/VersionID")]),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn explain_shows_an_index_read_as_a_plain_scan_node() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: PERSON_BOOKS.into(),
            ..Default::default()
        }
        .into(),
        capture_version("Person", "person_version").into(),
        Request {
            request: r#"query @explain {
                Book(filter: {author: {name: {_like: "%vid%"}}}) { title }
            }"#
            .into(),
            expected_data: json!(m::explain_target(
                "scanNode",
                false,
                1,
                json!({
                    "collectionID": m::r("person_version"),
                    "collectionName": "Person",
                    "filter": { "name": { "_like": "%vid%" } },
                    "prefixes": ["/1"],
                }),
            )),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
