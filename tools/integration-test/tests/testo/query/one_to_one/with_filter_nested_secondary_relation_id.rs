//! Secondary relation ID conditions nested inside logical filters.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn setup() -> Vec<Actions> {
    vec![
        AddCollection {
            sdl: "type Book { title: String publisher: Publisher } \
                  type Publisher { name: String book: Book @primary company: Company } \
                  type Company { name: String publisher: Publisher @primary }"
                .into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({"title": "Linked"})),
            common: Common {
                capture: captures([("book", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({"title": "Orphan 1"})),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({"title": "Orphan 2"})),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Publisher".into(),
            doc_map: Some(json!({"name": "Alice", "_bookID": m::r("book")})),
            common: Common {
                capture: captures([("publisher", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Company".into(),
            doc_map: Some(json!({"name": "Acme", "_publisherID": m::r("publisher")})),
            ..Default::default()
        }
        .into(),
    ]
}

fn run(request: Text, expected: Value) {
    let mut actions = setup();
    actions.push(
        Request {
            request,
            expected_data: expected,
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        ..TestCase::from_actions(actions)
    }
    .execute();
}

fn with_publisher(before: &'static str, after: &'static str) -> Text {
    Text::concat([
        Text::from(before),
        Text::reference("publisher"),
        Text::from(after),
    ])
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn or_with_secondary_relation_id_matches_related() {
    run(
        with_publisher(
            r#"query { Book(filter: {_or: [{_publisherID: {_eq: ""#,
            r#""}}]}) { title } }"#,
        ),
        json!({"Book": [{"title": "Linked"}]}),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn not_with_null_secondary_relation_id_matches_related() {
    run(
        Text::from(r#"query { Book(filter: {_not: {_publisherID: {_eq: null}}}) { title } }"#),
        json!({"Book": [{"title": "Linked"}]}),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn not_with_related_secondary_relation_id_matches_orphans() {
    run(
        with_publisher(
            r#"query { Book(filter: {_not: {_publisherID: {_eq: ""#,
            r#""}}}, order: {title: ASC}) { title } }"#,
        ),
        json!({"Book": [{"title": "Orphan 1"}, {"title": "Orphan 2"}]}),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn or_with_secondary_relation_id_and_sibling_matches_either() {
    run(
        with_publisher(
            r#"query { Book(filter: {_or: [{_publisherID: {_eq: ""#,
            r#""}}, {title: {_eq: "Orphan 2"}}]}, order: {title: ASC}) { title } }"#,
        ),
        json!({"Book": [{"title": "Linked"}, {"title": "Orphan 2"}]}),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn deeply_nested_secondary_relation_id_is_evaluated() {
    run(
        Text::from(
            r#"query { Book(filter: {_and: [
                {_or: [{_publisherID: {_eq: null}}, {title: {_eq: "Linked"}}]},
                {_not: {_and: [{title: {_eq: "Orphan 1"}}, {_publisherID: {_eq: null}}]}}
            ]}, order: {title: ASC}) { title } }"#,
        ),
        json!({"Book": [{"title": "Linked"}, {"title": "Orphan 2"}]}),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn nested_secondary_relation_id_keeps_selected_relation() {
    run(
        with_publisher(
            r#"query { Book(filter: {_or: [{_publisherID: {_eq: ""#,
            r#""}}, {title: {_eq: "Orphan 1"}}]}, order: {title: ASC}) {
                title _publisherID publisher { name } } }"#,
        ),
        json!({"Book": [
            {"title": "Linked", "_publisherID": m::r("publisher"), "publisher": {"name": "Alice"}},
            {"title": "Orphan 1", "_publisherID": null, "publisher": null},
        ]}),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn nested_secondary_relation_id_with_relation_object_filter() {
    run(
        Text::from(
            r#"query { Book(filter: {_or: [
                {_and: [{_publisherID: {_eq: null}}, {title: {_eq: "Orphan 2"}}]},
                {publisher: {name: {_eq: "Alice"}}}
            ]}, order: {title: ASC}) { title } }"#,
        ),
        json!({"Book": [{"title": "Linked"}, {"title": "Orphan 2"}]}),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn filtered_relation_selection_does_not_hide_the_parent_id() {
    run(
        with_publisher(
            r#"query { Book(filter: {_or: [{_publisherID: {_eq: ""#,
            r#""}}, {title: {_eq: "Orphan 1"}}]}, order: {title: ASC}) {
                title publisher(filter: {name: {_eq: "Bob"}}) { name } } }"#,
        ),
        json!({"Book": [
            {"title": "Linked", "publisher": null},
            {"title": "Orphan 1", "publisher": null},
        ]}),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn a_relation_path_in_a_dropped_logical_operator_still_gets_its_join() {
    run(
        Text::from(
            r#"query { Book(filter: {_or: [
                {_publisherID: {_eq: null}},
                {publisher: {company: {name: {_eq: "Acme"}}}}
            ]}, order: {title: ASC}) { title } }"#,
        ),
        json!({"Book": [
            {"title": "Linked"},
            {"title": "Orphan 1"},
            {"title": "Orphan 2"},
        ]}),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_one.nested_relation_id")]
fn and_keeps_its_scalar_conjunct_at_the_scan() {
    let mut actions = setup();
    actions.push(
        Request {
            request: with_publisher(
                r#"query { Book(filter: {_and: [{title: {_eq: "Linked"}}, {_publisherID: {_eq: ""#,
                r#""}}]}) { title } }"#,
            ),
            expected_data: json!({"Book": [{"title": "Linked"}]}),
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        Request {
            request: Text::from(
                r#"query { Book(filter: {_and: [{title: {_eq: "Orphan 1"}}, {_publisherID: {_eq: null}}]}) { title } }"#,
            ),
            expected_data: json!({"Book": [{"title": "Orphan 1"}]}),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        ..TestCase::from_actions(actions)
    }
    .execute();
}
