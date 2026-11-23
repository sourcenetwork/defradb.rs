//! An indexed relation field that orders only after a plain field does not
//! drive the join, as Go only inverts on the first order condition.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn a_relation_order_after_a_field_order_keeps_the_field_order() {
    let book = |n: usize, name: &str, rating: f64| -> integration_test::actions::Actions {
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({ "name": name, "rating": rating })),
            common: Common {
                capture: captures([(format!("book{n}").as_str(), "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into()
    };
    let publisher = |n: usize, year: i64| -> integration_test::actions::Actions {
        AddDoc {
            collection: "Publisher".into(),
            doc_map: Some(json!({ "_bookID": m::r(&format!("book{n}")), "yearOpened": year })),
            ..Default::default()
        }
        .into()
    };
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Book { name: String  rating: Float  publisher: Publisher } \
                  type Publisher { yearOpened: Int @index  book: Book @primary }"
                .into(),
            ..Default::default()
        }
        .into(),
        book(0, "Sooley", 3.2),
        book(1, "Theif Lord", 4.8),
        book(2, "The Rooster Bar", 4.0),
        publisher(0, 1999),
        publisher(1, 2020),
        publisher(2, 2022),
        Request {
            request:
                "query { Book(order: [{rating: ASC}, {publisher: {yearOpened: DESC}}]) { name } }"
                    .into(),
            expected_data: json!({ "Book": [
                { "name": "Sooley" },
                { "name": "The Rooster Bar" },
                { "name": "Theif Lord" },
            ] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
