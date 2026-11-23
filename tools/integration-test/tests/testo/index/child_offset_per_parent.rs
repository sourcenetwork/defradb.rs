//! A child selection's offset skips each parent's children, not the head of
//! the whole child index scan.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many.child-order-paging")]
fn a_child_offset_over_an_ordering_index_skips_per_parent() {
    let mut actions = vec![AddCollection {
        sdl: r#"
            type Book { name: String @index  author: Author }
            type Author { name: String  published: [Book] }
        "#
        .into(),
        ..Default::default()
    }
    .into()];
    for (name, id) in [("Cornelia Funke", "funke"), ("John Grisham", "grisham")] {
        actions.push(
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": name })),
                common: Common {
                    capture: captures([(id, "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
    }
    for (name, author) in [
        ("A Time for Mercy", "grisham"),
        ("Inkheart", "funke"),
        ("Painted House", "grisham"),
        ("Sooley", "grisham"),
        ("Theif Lord", "funke"),
    ] {
        actions.push(
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": name, "author": m::r(author) })),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        Request {
            request: "query { Author(order: {name: ASC}) { name \
                      published(order: {name: ASC}, limit: 1, offset: 1) { name } } }"
                .into(),
            expected_data: json!({ "Author": [
                { "name": "Cornelia Funke", "published": [{ "name": "Theif Lord" }] },
                { "name": "John Grisham", "published": [{ "name": "Painted House" }] },
            ] }),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}
