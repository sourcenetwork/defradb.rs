//! An index that only orders a page stops early only when nothing after the
//! scan drops or merges rows.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn add_doc(collection: &str, doc: Value, capture: Option<&str>) -> Actions {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(doc),
        common: Common {
            capture: capture
                .map(|name| captures([(name, "/docID")]))
                .unwrap_or_default(),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn request(query: &str, expected: Value) -> Actions {
    Request {
        request: query.into(),
        expected_data: expected,
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("go.index.order.single-field")]
fn a_grouped_page_counts_groups_not_documents() {
    let mut actions = vec![AddCollection {
        sdl: "type Users { Name: String @index  Age: Int }".into(),
        ..Default::default()
    }
    .into()];
    actions.extend(
        [("John", 25), ("John", 32), ("Carlo", 55), ("Alice", 19)]
            .map(|(name, age)| add_doc("Users", json!({ "Name": name, "Age": age }), None)),
    );
    actions.push(request(
        "query { Users(groupBy: [Name], order: {Name: DESC}, limit: 2) { Name } }",
        json!({ "Users": [{ "Name": "John" }, { "Name": "Carlo" }] }),
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.index.order.single-field")]
fn a_relation_filter_runs_before_the_page_stops() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Book { name: String  rating: Int @index  author: Author } \
                  type Author { name: String  books: [Book] }"
                .into(),
            ..Default::default()
        }
        .into(),
        add_doc("Author", json!({ "name": "A" }), Some("a")),
        add_doc("Author", json!({ "name": "B" }), Some("b")),
        add_doc(
            "Book",
            json!({ "name": "x", "rating": 1, "_authorID": m::r("a") }),
            None,
        ),
        add_doc(
            "Book",
            json!({ "name": "y", "rating": 2, "_authorID": m::r("b") }),
            None,
        ),
        request(
            "query { Book(filter: {author: {name: {_eq: \"B\"}}}, \
             order: {rating: ASC}, limit: 1) { name } }",
            json!({ "Book": [{ "name": "y" }] }),
        ),
    ])
    .execute();
}
