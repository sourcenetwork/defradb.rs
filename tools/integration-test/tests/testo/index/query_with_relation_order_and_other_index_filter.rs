//! A relation order on one child field keeps its sort when a filter picks the
//! child index of another field.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const PUBLISHERS: [(&str, i64, i64); 3] = [("Ann", 1, 9), ("Bob", 2, 2), ("Cid", 3, 5)];

fn case(index: &str, request: &str, expected: Value) {
    let mut actions: Vec<Actions> = vec![AddCollection {
        sdl: format!(
            "type Book {{ name: String  publisher: Publisher }} \
             type Publisher {{ rank: Int {index}  score: Int {index}  book: Book @primary }}"
        )
        .into(),
        ..Default::default()
    }
    .into()];
    for (n, (name, _, _)) in PUBLISHERS.iter().enumerate() {
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
    for (n, (_, rank, score)) in PUBLISHERS.iter().enumerate() {
        actions.push(
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r(&format!("book{n}")), "rank": rank, "score": score }),
                ),
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
    let books: Vec<Value> = names
        .iter()
        .map(|name| {
            let (_, rank, score) = PUBLISHERS.iter().find(|(n, _, _)| n == name).unwrap();
            json!({ "name": name, "publisher": { "rank": rank, "score": score } })
        })
        .collect();
    json!({ "Book": books })
}

const BY_SCORE: &str = "query { Book(filter: {publisher: {rank: {_gt: 0}}}, \
     order: {publisher: {score: ASC}}) { name publisher { rank score } } }";
const BY_SCORE_PAGE: &str = "query { Book(filter: {publisher: {rank: {_gt: 0}}}, \
     order: {publisher: {score: ASC}}, limit: 2) { name publisher { rank score } } }";
const BY_RANK_DESC: &str = "query { Book(filter: {publisher: {rank: {_gt: 0}}}, \
     order: {publisher: {rank: DESC}}) { name publisher { rank score } } }";
const BY_RANK_DESC_PAGE: &str = "query { Book(filter: {publisher: {rank: {_gt: 0}}}, \
     order: {publisher: {rank: DESC}}, limit: 2) { name publisher { rank score } } }";

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn order_on_another_field_than_the_filter_index_with_an_index() {
    case("@index", BY_SCORE, books(&["Bob", "Cid", "Ann"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn order_on_another_field_than_the_filter_index_without_an_index() {
    case("", BY_SCORE, books(&["Bob", "Cid", "Ann"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn a_page_ordered_on_another_field_than_the_filter_index_with_an_index() {
    case("@index", BY_SCORE_PAGE, books(&["Bob", "Cid"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn a_page_ordered_on_another_field_than_the_filter_index_without_an_index() {
    case("", BY_SCORE_PAGE, books(&["Bob", "Cid"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn descending_order_on_the_filter_index_with_an_index() {
    case("@index", BY_RANK_DESC, books(&["Cid", "Bob", "Ann"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn descending_order_on_the_filter_index_without_an_index() {
    case("", BY_RANK_DESC, books(&["Cid", "Bob", "Ann"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn a_descending_page_on_the_filter_index_with_an_index() {
    case("@index", BY_RANK_DESC_PAGE, books(&["Cid", "Bob"]));
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn a_descending_page_on_the_filter_index_without_an_index() {
    case("", BY_RANK_DESC_PAGE, books(&["Cid", "Bob"]));
}
