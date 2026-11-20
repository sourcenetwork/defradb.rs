//! Explain shows a read through an index as Go's plain scan node, with its
//! filter and prefixes.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn explain_shows_an_index_read_as_a_plain_scan_node() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: r#"
                type Person {
                    name: String @index
                    authoredBooks: [Book]
                }
                type Book {
                    title: String
                    author: Person
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
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
                    "collectionID": "bafyreifilnntrughum4p63ntvocxvwqg2eveymltpziwk7lluvncjftula",
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
