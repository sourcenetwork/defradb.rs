//! A JSON index answers a filter only when it answers it as a full scan does:
//! each filter runs on an indexed and an unindexed collection with the same
//! documents and must return the same names.

use integration_test::actions::{Actions, AddCollection, AddDoc, DeleteDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn stored() -> Vec<(&'static str, Value)> {
    vec![
        ("Obj", json!([{ "a": 1 }])),
        ("Nested", json!([[1]])),
        ("NestedEmpty", json!([[]])),
        ("One", json!([1])),
        ("Five", json!([5])),
        ("Empty", json!([])),
        ("TagsEmpty", json!({ "tags": [] })),
        ("Tags1", json!({ "tags": [1] })),
        ("TagsObj", json!({ "tags": [{ "a": 1 }] })),
        ("Null", Value::Null),
    ]
}

fn add(collection: &str, name: &str, custom: Value) -> Actions {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(json!({ "name": name, "custom": custom })),
        ..Default::default()
    }
    .into()
}

fn query(collection: &str, filter: &str, names: &[&str]) -> Actions {
    let names: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    Request {
        request: format!(
            "query {{ {collection}(filter: {filter}, order: {{name: ASC}}) {{ name }} }}"
        )
        .into(),
        expected_data: json!({ collection: names }),
        ..Default::default()
    }
    .into()
}

fn explain(filter: &str, fetches: u64) -> Actions {
    Request {
        request: format!("query @explain(type: execute) {{ Idx(filter: {filter}) {{ name }} }}")
            .into(),
        expected_data: json!(m::explain_scan(json!({ "indexFetches": fetches }))),
        ..Default::default()
    }
    .into()
}

fn collections(sdl_idx: &str, sdl_raw: &str) -> Vec<Actions> {
    [sdl_idx, sdl_raw]
        .into_iter()
        .map(|sdl| {
            AddCollection {
                sdl: sdl.into(),
                ..Default::default()
            }
            .into()
        })
        .collect()
}

fn paired_with(
    sdl_idx: &str,
    sdl_raw: &str,
    docs: Vec<(&'static str, Value)>,
    filters: &[(&str, &[&str])],
    extra: Vec<Actions>,
) {
    let mut actions = collections(sdl_idx, sdl_raw);
    for (name, custom) in docs {
        actions.push(add("Idx", name, custom.clone()));
        actions.push(add("Raw", name, custom));
    }
    actions.extend(extra);
    for (filter, names) in filters {
        actions.push(query("Idx", filter, names));
        actions.push(query("Raw", filter, names));
    }
    TestCase::from_actions(actions).execute();
}

fn paired(filters: &[(&str, &[&str])]) {
    paired_with(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
        stored(),
        filters,
        vec![],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn ne_under_all_matches_a_full_scan() {
    paired(&[
        (
            "{custom: {_all: {_ne: 5}}}",
            &["Empty", "Nested", "NestedEmpty", "Obj", "One"],
        ),
        (
            "{custom: {_all: {_nin: [5]}}}",
            &["Empty", "Nested", "NestedEmpty", "Obj", "One"],
        ),
        (
            "{custom: {_all: {_nlike: \"x%\"}}}",
            &["Empty", "Five", "Nested", "NestedEmpty", "Obj", "One"],
        ),
        (
            "{custom: {_any: {_ne: 5}}}",
            &["Nested", "NestedEmpty", "Obj", "One"],
        ),
    ]);
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn nested_array_operators_match_a_full_scan() {
    paired(&[
        (
            "{custom: {_all: {_all: {_eq: 1}}}}",
            &["Empty", "Nested", "NestedEmpty"],
        ),
        (
            "{custom: {_any: {_all: {_eq: 1}}}}",
            &["Nested", "NestedEmpty"],
        ),
        (
            "{custom: {_none: {_all: {_eq: 1}}}}",
            &["Empty", "Five", "Obj", "One"],
        ),
    ]);
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn bare_negatives_match_a_full_scan() {
    paired(&[
        (
            "{custom: {_ne: 5}}",
            &[
                "Empty",
                "Five",
                "Nested",
                "NestedEmpty",
                "Null",
                "Obj",
                "One",
                "Tags1",
                "TagsEmpty",
                "TagsObj",
            ],
        ),
        (
            "{custom: {_nin: [5]}}",
            &[
                "Empty",
                "Five",
                "Nested",
                "NestedEmpty",
                "Null",
                "Obj",
                "One",
                "Tags1",
                "TagsEmpty",
                "TagsObj",
            ],
        ),
        (
            "{custom: {tags: {_ne: 5}}}",
            &["Tags1", "TagsEmpty", "TagsObj"],
        ),
        (
            "{custom: {tags: {_nin: [5]}}}",
            &["Null", "Obj", "Tags1", "TagsEmpty", "TagsObj"],
        ),
    ]);
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn positive_all_and_any_match_a_full_scan() {
    paired(&[
        ("{custom: {_all: {_eq: 1}}}", &["Empty", "One"]),
        ("{custom: {_all: {_in: [1, 5]}}}", &["Empty", "Five", "One"]),
        ("{custom: {_all: {_gt: 0}}}", &["Empty", "Five", "One"]),
        ("{custom: {_any: {_eq: 1}}}", &["One"]),
    ]);
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn array_operators_on_a_path_match_a_full_scan() {
    paired(&[(
        "{custom: {tags: {_all: {_eq: 1}}}}",
        &["Tags1", "TagsEmpty"],
    )]);
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn an_object_filter_under_all_fails_with_and_without_the_index() {
    let mut actions = collections(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
    );
    for collection in ["Idx", "Raw"] {
        for (name, custom) in stored() {
            actions.push(add(collection, name, custom));
        }
        for filter in [
            "{custom: {tags: {_all: {a: {_eq: 1}}}}}",
            "{custom: {_all: {a: {_eq: 1}}}}",
        ] {
            actions.push(
                Request {
                    request: format!("query {{ {collection}(filter: {filter}) {{ name }} }}")
                        .into(),
                    expected_error: Some("unknown operator in array condition: a".into()),
                    ..Default::default()
                }
                .into(),
            );
        }
    }
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn the_index_serves_positive_all_and_not_negative_all() {
    let mut actions = collections(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
    );
    for (name, custom) in stored() {
        actions.push(add("Idx", name, custom));
    }
    actions.push(explain("{custom: {_all: {_eq: 1}}}", 2));
    actions.push(explain("{custom: {tags: {_all: {_eq: 1}}}}", 2));
    actions.push(explain("{custom: {_all: {_ne: 5}}}", 0));
    TestCase::from_actions(actions).execute();
}

fn tags_docs() -> Vec<(&'static str, Value)> {
    vec![
        ("Empty", json!({ "tags": [] })),
        ("One", json!({ "tags": [1] })),
        ("Two", json!({ "tags": [2] })),
        ("Missing", json!({ "other": 1 })),
    ]
}

const TAGS_FILTERS: &[(&str, &[&str])] = &[
    ("{custom: {tags: {_all: {_eq: 1}}}}", &["Empty", "One"]),
    ("{custom: {tags: {_ne: 5}}}", &["Empty", "One", "Two"]),
];

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn a_composite_json_index_matches_a_full_scan() {
    paired_with(
        r#"type Idx @index(includes: [{field: "custom"}, {field: "name"}]) { name: String  custom: JSON }"#,
        "type Raw { name: String  custom: JSON }",
        tags_docs(),
        TAGS_FILTERS,
        vec![],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn a_descending_json_index_matches_a_full_scan() {
    paired_with(
        "type Idx { name: String  custom: JSON @index(direction: DESC) }",
        "type Raw { name: String  custom: JSON }",
        tags_docs(),
        TAGS_FILTERS,
        vec![],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn a_unique_json_index_matches_a_full_scan() {
    paired_with(
        "type Idx { name: String  custom: JSON @index(unique: true) }",
        "type Raw { name: String  custom: JSON }",
        tags_docs(),
        TAGS_FILTERS,
        vec![],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn updates_and_deletes_keep_the_index_matching_a_full_scan() {
    let mut actions = collections(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
    );
    let docs = [
        ("Grows", json!({ "tags": [] })),
        ("Shrinks", json!({ "tags": [1] })),
        ("Gone", json!({ "tags": [] })),
    ];
    for collection in ["Idx", "Raw"] {
        for (name, custom) in &docs {
            actions.push(
                AddDoc {
                    collection: collection.into(),
                    doc_map: Some(json!({ "name": name, "custom": custom })),
                    common: Common {
                        capture: captures([(key(collection, name), "/docID")]),
                        ..Default::default()
                    },
                    ..Default::default()
                }
                .into(),
            );
        }
    }
    let both = |actions: &mut Vec<Actions>, all: &[&str], ne: &[&str]| {
        for collection in ["Idx", "Raw"] {
            actions.push(query(collection, "{custom: {tags: {_all: {_eq: 1}}}}", all));
            actions.push(query(collection, "{custom: {tags: {_ne: 5}}}", ne));
        }
    };
    both(
        &mut actions,
        &["Gone", "Grows", "Shrinks"],
        &["Gone", "Grows", "Shrinks"],
    );
    for (name, tags) in [("Grows", json!([2])), ("Shrinks", json!([]))] {
        for collection in ["Idx", "Raw"] {
            actions.push(
                UpdateDoc {
                    collection: collection.into(),
                    doc_id: Text::reference(key(collection, name)),
                    doc_map: json!({ "custom": { "tags": tags } }),
                    expected_data: json!({}),
                    ..Default::default()
                }
                .into(),
            );
        }
        let (all, ne): (&[&str], &[&str]) = (&["Gone", "Shrinks"], &["Gone", "Grows", "Shrinks"]);
        both(&mut actions, all, ne);
    }
    for collection in ["Idx", "Raw"] {
        actions.push(
            DeleteDoc {
                collection: collection.into(),
                doc_id: Text::reference(key(collection, "Gone")),
                ..Default::default()
            }
            .into(),
        );
    }
    both(&mut actions, &["Shrinks"], &["Grows", "Shrinks"]);
    TestCase::from_actions(actions).execute();
}

fn key(collection: &str, name: &str) -> &'static str {
    Box::leak(format!("{collection}{name}").into_boxed_str())
}
