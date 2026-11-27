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
    add_fields(collection, name, json!({ "custom": custom }))
}

fn add_fields(collection: &str, name: &str, fields: Value) -> Actions {
    let mut doc = json!({ "name": name });
    if let (Some(doc), Some(fields)) = (doc.as_object_mut(), fields.as_object()) {
        doc.extend(fields.clone());
    }
    AddDoc {
        collection: collection.into(),
        doc_map: Some(doc),
        ..Default::default()
    }
    .into()
}

fn query(collection: &str, filter: &str, names: &[&str]) -> Actions {
    query_with(collection, filter, "order: {name: ASC}", names)
}

fn query_with(collection: &str, filter: &str, args: &str, names: &[&str]) -> Actions {
    let names: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    Request {
        request: format!("query {{ {collection}(filter: {filter}, {args}) {{ name }} }}").into(),
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
    let docs = docs
        .into_iter()
        .map(|(name, custom)| (name, json!({ "custom": custom })))
        .collect();
    paired_fields(sdl_idx, sdl_raw, docs, filters, extra);
}

fn paired_fields(
    sdl_idx: &str,
    sdl_raw: &str,
    docs: Vec<(&'static str, Value)>,
    filters: &[(&str, &[&str])],
    extra: Vec<Actions>,
) {
    let mut actions = collections(sdl_idx, sdl_raw);
    for (name, fields) in docs {
        actions.push(add_fields("Idx", name, fields.clone()));
        actions.push(add_fields("Raw", name, fields));
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
#[mantra_macros::req_test("defra.index.json.matches-scan")]
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
#[mantra_macros::req_test("defra.index.json.matches-scan")]
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
#[mantra_macros::req_test("defra.index.json.matches-scan")]
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
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn positive_all_and_any_match_a_full_scan() {
    paired(&[
        ("{custom: {_all: {_eq: 1}}}", &["Empty", "One"]),
        ("{custom: {_all: {_in: [1, 5]}}}", &["Empty", "Five", "One"]),
        ("{custom: {_all: {_gt: 0}}}", &["Empty", "Five", "One"]),
        ("{custom: {_any: {_eq: 1}}}", &["One"]),
    ]);
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn array_operators_on_a_path_match_a_full_scan() {
    paired(&[(
        "{custom: {tags: {_all: {_eq: 1}}}}",
        &["Tags1", "TagsEmpty"],
    )]);
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn an_object_filter_under_all_fails_with_and_without_the_index() {
    let mut actions = collections(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
    );
    // No element holds `a`, so only the empty array would come back.
    let docs: Vec<_> = stored()
        .into_iter()
        .filter(|(name, _)| *name != "TagsObj")
        .collect();
    for collection in ["Idx", "Raw"] {
        for (name, custom) in docs.clone() {
            actions.push(add(collection, name, custom));
        }
        for filter in [
            "{custom: {tags: {_all: {a: {_eq: 1}}}}}",
            "{custom: {tags: {_any: {a: {_eq: 1}}}}}",
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
#[mantra_macros::req_test("defra.index.json.matches-scan")]
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
    ("{custom: {tags: {_all: {_gt: 1}}}}", &["Empty", "Two"]),
    ("{custom: {tags: {_all: {_lte: 1}}}}", &["Empty", "One"]),
    ("{custom: {tags: {_all: {_in: [2, 3]}}}}", &["Empty", "Two"]),
    ("{custom: {tags: {_any: {_lt: 2}}}}", &["One"]),
    ("{custom: {tags: {_any: {_in: [1, 2]}}}}", &["One", "Two"]),
    (
        "{custom: {tags: {_all: {_eq: 1}}}, name: {_eq: \"One\"}}",
        &["One"],
    ),
    (
        "{custom: {tags: {_all: {_eq: 1}}}, name: {_eq: \"Empty\"}}",
        &["Empty"],
    ),
];

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn a_composite_json_index_matches_a_full_scan() {
    paired_with(
        r#"type Idx @index(includes: [{field: "custom"}, {field: "name"}]) { name: String  custom: JSON }"#,
        "type Raw { name: String  custom: JSON }",
        tags_docs(),
        TAGS_FILTERS,
        vec![
            explain(
                "{custom: {tags: {_all: {_eq: 1}}}, name: {_eq: \"One\"}}",
                2,
            ),
            explain("{custom: {tags: {_all: {_gt: 1}}}}", 2),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn a_descending_json_index_matches_a_full_scan() {
    paired_with(
        "type Idx { name: String  custom: JSON @index(direction: DESC) }",
        "type Raw { name: String  custom: JSON }",
        tags_docs(),
        TAGS_FILTERS,
        vec![
            explain("{custom: {tags: {_all: {_gt: 1}}}}", 2),
            explain("{custom: {tags: {_all: {_in: [2, 3]}}}}", 2),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
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
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn updates_and_deletes_keep_the_index_matching_a_full_scan() {
    updates_and_deletes_match_a_full_scan("type Idx { name: String  custom: JSON @index }");
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn updates_and_deletes_keep_a_unique_json_index_matching_a_full_scan() {
    updates_and_deletes_match_a_full_scan(
        "type Idx { name: String  custom: JSON @index(unique: true) }",
    );
}

fn updates_and_deletes_match_a_full_scan(sdl_idx: &str) {
    let mut actions = collections(sdl_idx, "type Raw { name: String  custom: JSON }");
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

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn a_pattern_from_another_any_clause_matches_a_full_scan() {
    paired_with(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
        vec![
            ("Mixed", json!({ "tags": [5, "xa"] })),
            ("Five", json!({ "tags": [5] })),
            ("Xa", json!({ "tags": ["xa"] })),
        ],
        &[
            (
                "{_and: [{custom: {tags: {_any: {_eq: 5}}}}, {custom: {tags: {_any: {_like: \"x%\"}}}}]}",
                &["Mixed"],
            ),
            (
                "{_and: [{custom: {tags: {_any: {_lt: 100}}}}, {custom: {tags: {_any: {_like: \"x%\"}}}}]}",
                &["Mixed"],
            ),
        ],
        vec![explain(
            "{_and: [{custom: {tags: {_any: {_eq: 5}}}}, {custom: {tags: {_any: {_like: \"x%\"}}}}]}",
            2,
        )],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn a_pattern_from_another_any_clause_on_a_typed_array_matches_a_full_scan() {
    paired_fields(
        "type Idx { name: String  tags: [String] @index }",
        "type Raw { name: String  tags: [String] }",
        vec![
            ("Mixed", json!({ "tags": ["a", "xa"] })),
            ("A", json!({ "tags": ["a"] })),
        ],
        &[(
            "{_and: [{tags: {_any: {_eq: \"a\"}}}, {tags: {_any: {_like: \"x%\"}}}]}",
            &["Mixed"],
        )],
        vec![],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn an_in_listing_an_object_matches_a_full_scan() {
    paired_with(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
        vec![
            ("Elem", json!({ "tags": [{ "x": 1 }] })),
            ("One", json!({ "tags": [1] })),
            ("Bare", json!({ "tags": { "x": 1 } })),
        ],
        &[
            (
                "{custom: {tags: {_all: {_in: [1, {x: 1}]}}}}",
                &["Elem", "One"],
            ),
            (
                "{custom: {tags: {_any: {_in: [1, {x: 1}]}}}}",
                &["Elem", "One"],
            ),
            ("{custom: {tags: {_in: [1, {x: 1}]}}}", &["Bare"]),
        ],
        vec![explain("{custom: {tags: {_all: {_in: [1, {x: 1}]}}}}", 0)],
    );
}

const COMPOSITE_AGE_IDX: &str = r#"type Idx @index(includes: [{field: "age"}, {field: "custom"}]) { name: String  age: Int  custom: JSON }"#;
const COMPOSITE_AGE_RAW: &str = "type Raw { name: String  age: Int  custom: JSON }";

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn a_json_field_second_in_a_composite_index_matches_a_full_scan() {
    paired_fields(
        COMPOSITE_AGE_IDX,
        COMPOSITE_AGE_RAW,
        vec![
            ("Deep", json!({ "age": 25, "custom": { "tags": [[2]] } })),
            ("Title", json!({ "age": 25, "custom": { "title": "Dr" } })),
            ("Other", json!({ "age": 30, "custom": { "tags": [[2]] } })),
        ],
        &[
            (
                "{age: {_eq: 25}, custom: {tags: {_none: {_any: {_eq: 1}}}}}",
                &["Deep"],
            ),
            (
                "{age: {_eq: 25}, custom: {tags: {_all: {_any: {_eq: 2}}}}}",
                &["Deep"],
            ),
            ("{age: {_eq: 25}, custom: {title: {_eq: null}}}", &["Deep"]),
        ],
        vec![
            explain(
                "{age: {_eq: 25}, custom: {tags: {_none: {_any: {_eq: 1}}}}}",
                0,
            ),
            explain("{age: {_eq: 25}, custom: {title: {_eq: null}}}", 0),
        ],
    );
    paired_fields(
        COMPOSITE_AGE_IDX,
        COMPOSITE_AGE_RAW,
        vec![
            ("Three", json!({ "age": 25, "custom": 3 })),
            ("ArrThree", json!({ "age": 25, "custom": [3] })),
            ("Other", json!({ "age": 30, "custom": 3 })),
        ],
        &[
            ("{age: {_eq: 25}, custom: {_eq: 3}}", &["Three"]),
            ("{age: {_eq: 25}, custom: {_any: {_eq: 3}}}", &["ArrThree"]),
            (
                "{age: {_in: [25, 30]}, custom: {_eq: 3}}",
                &["Other", "Three"],
            ),
        ],
        vec![
            explain("{age: {_eq: 25}, custom: {_eq: 3}}", 1),
            explain("{age: {_eq: 25}, custom: {_any: {_eq: 3}}}", 1),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn null_checks_at_a_path_match_a_full_scan() {
    paired_with(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
        vec![
            ("Null", json!({ "title": null })),
            ("Missing", json!({ "other": 1 })),
            ("Dr", json!({ "title": "Dr" })),
            ("Top", Value::Null),
        ],
        &[
            (
                "{custom: {title: {_eq: null}}}",
                &["Missing", "Null", "Top"],
            ),
            (
                "{custom: {title: {_in: [null, \"Dr\"]}}}",
                &["Dr", "Missing", "Null", "Top"],
            ),
            ("{custom: {_in: [null, 5]}}", &["Top"]),
            ("{custom: {_eq: null}}", &["Top"]),
        ],
        vec![
            explain("{custom: {title: {_eq: null}}}", 0),
            explain("{custom: {title: {_in: [null, \"Dr\"]}}}", 0),
            explain("{custom: {_in: [null, 5]}}", 1),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn the_index_serves_positive_filters_and_not_its_fallbacks() {
    let mut actions = collections(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
    );
    let mut docs = stored();
    docs.push(("TagsXa", json!({ "tags": ["xa"] })));
    for (name, custom) in docs {
        actions.push(add("Idx", name, custom));
    }
    for (filter, fetches) in [
        ("{custom: {_any: {_eq: 1}}}", 1),
        ("{custom: {tags: {_any: {_eq: 1}}}}", 1),
        // The range reaches the strings, which sort after the numbers.
        ("{custom: {tags: {_any: {_gt: 0}}}}", 2),
        ("{custom: {tags: {_any: {_like: \"x%\"}}}}", 2),
        ("{custom: {tags: {_any: {_in: [1, 5]}}}}", 1),
        ("{custom: {_any: {_in: [5, 1]}}}", 2),
        ("{custom: {tags: {_all: {_in: [1, {x: 1}]}}}}", 0),
        ("{custom: {title: {_eq: null}}}", 0),
        ("{custom: {_all: {_any: {_eq: 1}}}}", 0),
    ] {
        actions.push(explain(filter, fetches));
    }
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn all_with_a_page_matches_a_full_scan() {
    let docs = vec![
        ("A", json!({ "tags": [] })),
        ("B", json!({ "tags": [1] })),
        ("C", json!({ "tags": [1, 1] })),
        ("D", json!({ "tags": [2] })),
        ("E", json!({ "tags": [], "z": 1 })),
    ];
    let filter = "{custom: {tags: {_all: {_eq: 1}}}}";
    let mut actions = collections(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
    );
    for (name, custom) in docs {
        actions.push(add("Idx", name, custom.clone()));
        actions.push(add("Raw", name, custom));
    }
    for collection in ["Idx", "Raw"] {
        let pages: [(&str, &[&str]); 4] = [
            ("order: {name: ASC}, limit: 2, offset: 1", &["B", "C"]),
            ("order: {name: DESC}, limit: 2", &["E", "C"]),
            ("order: {custom: ASC}, limit: 3", &["C", "B", "E"]),
            ("order: {custom: DESC}, limit: 2, offset: 1", &["E", "B"]),
        ];
        for (args, names) in pages {
            actions.push(query_with(collection, filter, args, names));
        }
    }
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn any_beside_all_on_one_path_matches_a_full_scan() {
    let mut docs = tags_docs();
    docs.push(("OneTwo", json!({ "tags": [1, 2] })));
    paired_with(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
        docs,
        &[
            (
                "{custom: {tags: {_all: {_eq: 1}, _any: {_eq: 1}}}}",
                &["One"],
            ),
            (
                "{custom: {tags: {_all: {_gt: 0}, _any: {_lt: 2}}}}",
                &["One", "OneTwo"],
            ),
            (
                "{_and: [{custom: {tags: {_any: {_eq: 1}}}}, {custom: {tags: {_all: {_eq: 1}}}}]}",
                &["One"],
            ),
        ],
        vec![
            // The marker of `[]` is not read: `_any` fails an empty array.
            explain("{custom: {tags: {_all: {_eq: 1}, _any: {_eq: 1}}}}", 2),
        ],
    );
    paired_with(
        "type Idx { name: String  custom: JSON @index }",
        "type Raw { name: String  custom: JSON }",
        vec![
            ("AEmpty", json!({ "a": [], "b": [2] })),
            ("BEmpty", json!({ "a": [1], "b": [] })),
        ],
        &[(
            "{custom: {a: {_all: {_eq: 1}}, b: {_any: {_eq: 2}}}}",
            &["AEmpty"],
        )],
        vec![],
    );
}
