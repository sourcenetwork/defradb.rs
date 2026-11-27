//! A relation order whose index only covers the first condition still sorts
//! the ties by the later ones.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const NAMES: [&str; 5] = ["Ann", "Bob", "Cid", "Dee", "Eve"];

fn case(index: &str, request: &str, expected: Value) {
    let mut actions: Vec<Actions> = vec![AddCollection {
        sdl: format!(
            "type Book {{ name: String  publisher: Publisher }} \
             type Publisher {{ year: Int {index}  book: Book @primary }}"
        )
        .into(),
        ..Default::default()
    }
    .into()];
    for (n, name) in NAMES.iter().enumerate() {
        actions.push(
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": name })),
                common: Common {
                    capture: captures([(format!("book{n}").as_str(), "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
    }
    for n in 0..NAMES.len() {
        let year = if n == 4 { 2001 } else { 2000 };
        actions.push(
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r(&format!("book{n}")), "year": year })),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        Request {
            request: request.into(),
            expected_data: expected,
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

fn books(names: &[&str]) -> Value {
    let names: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    json!({ "Book": names })
}

const ORDER: &str = "query { Book(order: [{publisher: {year: ASC}}, {name: DESC}]) { name } }";
const PAGE: &str =
    "query { Book(order: [{publisher: {year: ASC}}, {name: DESC}], limit: 2) { name } }";

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn ties_follow_the_second_condition_with_an_index() {
    case("@index", ORDER, books(&["Dee", "Cid", "Bob", "Ann", "Eve"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn ties_follow_the_second_condition_without_an_index() {
    case("", ORDER, books(&["Dee", "Cid", "Bob", "Ann", "Eve"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn a_page_of_ties_follows_the_second_condition_with_an_index() {
    case("@index", PAGE, books(&["Dee", "Cid"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn a_page_of_ties_follows_the_second_condition_without_an_index() {
    case("", PAGE, books(&["Dee", "Cid"]));
}
