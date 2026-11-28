//! Index regressions for relation aggregation and composite residual filters.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn relation_aggregation(indexed: bool) {
    let index = if indexed { " @index" } else { "" };
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: format!(
                    r#"
                    type Foo {{
                        name: String
                        bars: [Bar]
                    }}

                    type Bar {{
                        points: Int
                        foo: Foo{index}
                    }}
                    "#
                )
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Foo".into(),
                doc_map: Some(json!({ "name": "target" })),
                common: Common {
                    capture: captures([("target", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Foo".into(),
                doc_map: Some(json!({ "name": "other" })),
                common: Common {
                    capture: captures([("other", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Bar".into(),
                doc_map: Some(json!({ "points": 10, "_fooID": m::r("target") })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Bar".into(),
                doc_map: Some(json!({ "points": 20, "_fooID": m::r("target") })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Bar".into(),
                doc_map: Some(json!({ "points": 50, "_fooID": m::r("other") })),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from("query { Bar(groupBy: [_fooID], filter: {_fooID: {_eq: \""),
                    Text::reference("target"),
                    Text::from("\"}}) { foo { _docID } total: SUM(GROUP: {field: points}) } }"),
                ]),
                expected_data: json!({
                    "Bar": [{
                        "foo": { "_docID": m::r("target") },
                        "total": 30,
                    }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.group-aggregate")]
fn relation_aggregation_through_an_indexed_fk_matches_the_unindexed_query() {
    relation_aggregation(true);
    relation_aggregation(false);
}

fn none_on_second_composite_field(indexed: bool) {
    let index = if indexed {
        r#"@index(includes: [{field: "name"}, {field: "numbers"}])"#
    } else {
        ""
    };
    let expected = json!({
        "User": [
            { "name": "Shahzad", "numbers": [] },
            { "name": "Shahzad", "numbers": [1, 2] },
            { "name": "Shahzad", "numbers": [4] },
        ],
    });
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: format!(
                    r#"
                    type User {index} {{
                        name: String
                        numbers: [Int!]
                    }}
                    "#
                )
                .into(),
                ..Default::default()
            }
            .into(),
            add_user("Shahzad", json!([])).into(),
            add_user("Shahzad", json!([1, 2])).into(),
            add_user("Shahzad", json!([3])).into(),
            add_user("Shahzad", json!([3, 4])).into(),
            add_user("Shahzad", json!([4])).into(),
            add_user("Bruno", json!([1, 2])).into(),
            Request {
                request: r#"
                    query {
                        User(filter: {name: {_eq: "Shahzad"}, numbers: {_none: {_eq: 3}}}) {
                            name
                            numbers
                        }
                    }
                "#
                .into(),
                expected_data: expected,
                common: Common {
                    unordered_documents: vec!["/data/User".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

fn add_user(name: &str, numbers: Value) -> integration_test::actions::Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "numbers": numbers })),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.index.array.composite-none")]
fn none_on_the_second_composite_field_matches_the_unindexed_query() {
    none_on_second_composite_field(true);
    none_on_second_composite_field(false);
}
