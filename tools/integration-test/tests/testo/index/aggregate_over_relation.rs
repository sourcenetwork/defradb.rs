//! Aggregates over relations whose foreign key or fields are indexed return the
//! same results as the same query without the indexes (sourcenetwork/defradb#2829).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const INDEXED_SDL: &str = r#"
    type Foo {
        name: String @index(unique: true)
        bars: [Bar]
    }

    type Bar {
        points: Int @index
        foo: Foo @index
    }
"#;

const PLAIN_SDL: &str = r#"
    type Foo {
        name: String
        bars: [Bar]
    }

    type Bar {
        points: Int
        foo: Foo
    }
"#;

fn add_foo(name: &str, capture: &str) -> AddDoc {
    AddDoc {
        collection: "Foo".into(),
        doc_map: Some(json!({ "name": name })),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn add_bar(doc: Value) -> AddDoc {
    AddDoc {
        collection: "Bar".into(),
        doc_map: Some(doc),
        ..Default::default()
    }
}

fn scenario(id: &'static str, sdl: &str, request: Request) -> TestCase {
    TestCase {
        id,
        actions: vec![
            AddCollection {
                sdl: sdl.into(),
                ..Default::default()
            }
            .into(),
            add_foo("Alice", "alice").into(),
            add_foo("Bob", "bob").into(),
            add_foo("Carol", "carol").into(),
            add_bar(json!({ "points": 10, "_fooID": m::r("alice") })).into(),
            add_bar(json!({ "points": 20, "_fooID": m::r("alice") })).into(),
            add_bar(json!({ "points": 5, "_fooID": m::r("bob") })).into(),
            add_bar(json!({ "points": 7 })).into(),
            request.into(),
        ],
        ..Default::default()
    }
}

fn group_by_fk_with_fk_filter_text(directive: &str) -> Text {
    Text::concat([
        Text::from(format!(
            r#"query {directive} {{
                Bar(groupBy: [_fooID], filter: {{_fooID: {{_eq: ""#
        )),
        Text::reference("alice"),
        Text::from(
            r#""}}) {
                    _fooID
                    SUM(GROUP: {field: points})
                    COUNT(GROUP: {})
                }
            }"#,
        ),
    ])
}

fn group_by_indexed_fk_with_fk_filter() -> Request {
    Request {
        request: group_by_fk_with_fk_filter_text(""),
        expected_data: json!({
            "Bar": [{ "_fooID": m::r("alice"), "SUM": 30, "COUNT": 2 }],
        }),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate")]
fn group_by_indexed_fk_with_fk_filter_sums_the_group() {
    let mut test = scenario(
        integration_test::scenario_id!(),
        INDEXED_SDL,
        group_by_indexed_fk_with_fk_filter(),
    );
    test.actions.push(
        Request {
            request: group_by_fk_with_fk_filter_text("@explain(type: execute)"),
            expected_data: json!(m::explain_scan_at(
                json!({ "indexFetches": 2, "filterMatches": 2 }),
                json!(["root"])
            )),
            ..Default::default()
        }
        .into(),
    );
    test.execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate")]
fn group_by_unindexed_fk_with_fk_filter_sums_the_group() {
    scenario(
        integration_test::scenario_id!(),
        PLAIN_SDL,
        group_by_indexed_fk_with_fk_filter(),
    )
    .execute();
}

fn group_by_fk_with_related_filter() -> Request {
    Request {
        request: r#"
            query {
                Bar(groupBy: [_fooID], filter: {foo: {name: {_eq: "Bob"}}}) {
                    _fooID
                    COUNT(GROUP: {})
                    SUM(GROUP: {field: points})
                }
            }
        "#
        .into(),
        expected_data: json!({
            "Bar": [{ "_fooID": m::r("bob"), "COUNT": 1, "SUM": 5 }],
        }),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate")]
fn group_by_indexed_fk_with_indexed_related_filter_counts_the_group() {
    scenario(
        integration_test::scenario_id!(),
        INDEXED_SDL,
        group_by_fk_with_related_filter(),
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate")]
fn group_by_unindexed_fk_with_related_filter_counts_the_group() {
    scenario(
        integration_test::scenario_id!(),
        PLAIN_SDL,
        group_by_fk_with_related_filter(),
    )
    .execute();
}

fn parent_filter_with_child_aggregates() -> Request {
    Request {
        request: r#"
            query {
                Foo(filter: {name: {_eq: "Alice"}}) {
                    name
                    SUM(bars: {field: points})
                    AVG(bars: {field: points})
                }
            }
        "#
        .into(),
        expected_data: json!({
            "Foo": [{ "name": "Alice", "SUM": 30, "AVG": 15.0 }],
        }),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate")]
fn indexed_parent_filter_aggregates_children_through_indexed_fk() {
    scenario(
        integration_test::scenario_id!(),
        INDEXED_SDL,
        parent_filter_with_child_aggregates(),
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate")]
fn unindexed_parent_filter_aggregates_children() {
    scenario(
        integration_test::scenario_id!(),
        PLAIN_SDL,
        parent_filter_with_child_aggregates(),
    )
    .execute();
}

fn child_aggregates_with_child_filter() -> Request {
    Request {
        request: r#"
            query {
                Foo {
                    name
                    COUNT(bars: {filter: {points: {_gt: 8}}})
                    SUM(bars: {field: points, filter: {points: {_gt: 8}}})
                }
            }
        "#
        .into(),
        expected_data: json!({
            "Foo": [
                { "name": "Alice", "COUNT": 2, "SUM": 30 },
                { "name": "Bob", "COUNT": 0, "SUM": 0 },
                { "name": "Carol", "COUNT": 0, "SUM": 0 },
            ],
        }),
        common: Common {
            unordered_documents: vec!["/data/Foo".into()],
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate")]
fn child_aggregates_with_filter_on_indexed_child_field() {
    scenario(
        integration_test::scenario_id!(),
        INDEXED_SDL,
        child_aggregates_with_child_filter(),
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate")]
fn child_aggregates_with_filter_on_unindexed_child_field() {
    scenario(
        integration_test::scenario_id!(),
        PLAIN_SDL,
        child_aggregates_with_child_filter(),
    )
    .execute();
}
