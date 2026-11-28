//! An index dropped and created again while its collection is updated is
//! built and used (`defra.index.ddl_under_writes`).

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{
    Actions, AddCollection, AddDoc, DeleteIndex, GetCollections, ListIndexes, NewIndex, Parallel,
    PatchCollection, Request, SetActiveCollectionVersion, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const DOCS: usize = 4;
const ROUNDS: usize = 4;

fn succeeds() -> Value {
    m::object(json!({ "error": "" }))
}

fn doc(i: usize) -> String {
    format!("doc{i}")
}

fn add(i: usize) -> Actions {
    AddDoc {
        collection: "U".into(),
        doc_map: Some(json!({ "n": 0, "g": i, "h": i })),
        common: Common {
            capture: captures([(doc(i).as_str(), "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// Every document updated `ROUNDS` times, each time to its final `g` and `h`.
fn updates() -> Vec<Actions> {
    (0..ROUNDS)
        .flat_map(|_| 0..DOCS)
        .map(|i| {
            UpdateDoc {
                collection: "U".into(),
                doc_id: Text::reference(&doc(i)),
                doc_map: json!({ "n": 1, "g": 100 + i, "h": 100 + i }),
                expected_data: succeeds(),
                common: Common {
                    retry: true,
                    envelope: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into()
        })
        .collect()
}

fn index_on(name: &str, field: &str) -> IndexSpec {
    IndexSpec {
        name: name.into(),
        unique: false,
        fields: vec![IndexField {
            name: field.into(),
            descending: false,
        }],
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.index.ddl_under_writes")]
fn an_index_dropped_and_created_again_under_updates_is_built_and_used() {
    let mut drop_under_writes = updates();
    drop_under_writes.push(
        DeleteIndex {
            collection: "U".into(),
            index_name: "by_value".into(),
            expected_data: succeeds(),
            common: Common {
                envelope: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    );
    let mut create_under_writes = updates();
    create_under_writes.push(
        NewIndex {
            collection: "U".into(),
            index: index_on("by_value", "h"),
            expected_data: succeeds(),
            common: Common {
                envelope: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    );

    let mut actions: Vec<Actions> = vec![AddCollection {
        sdl: "type U { n: Int @crdt(type: pcounter)  g: Int  h: Int }".into(),
        ..Default::default()
    }
    .into()];
    actions.extend((0..DOCS).map(add));
    actions.extend([
        NewIndex {
            collection: "U".into(),
            index: index_on("by_value", "g"),
            ..Default::default()
        }
        .into(),
        Parallel {
            actions: drop_under_writes,
            ..Default::default()
        }
        .into(),
        ListIndexes {
            collection: "U".into(),
            expected_data: json!({ "indexes": m::unordered(json!([])) }),
            ..Default::default()
        }
        .into(),
        Parallel {
            actions: create_under_writes,
            ..Default::default()
        }
        .into(),
        ListIndexes {
            collection: "U".into(),
            expected_data: json!({
                "indexes": [m::object(json!({
                    "Description": m::object(json!({
                        "Name": "by_value",
                        "ID": 2,
                        "Fields": [m::object(json!({ "Name": "h" }))],
                    })),
                    "Execution": m::object(json!({ "Status": 3 })),
                }))],
            }),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { U(filter: {h: {_eq: 101}}) { h } }".into(),
            expected_data: json!({ "U": [{ "h": 101 }] }),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query @explain(type: execute) { U(filter: {h: {_eq: 101}}) { h } }".into(),
            expected_data: m::explain_scan(json!({ "indexFetches": 1 })),
            ..Default::default()
        }
        .into(),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.ddl_under_writes")]
fn indexes_created_together_under_updates_are_built() {
    let mut actions: Vec<Actions> = vec![AddCollection {
        sdl: "type U { n: Int @crdt(type: pcounter)  g: Int  h: Int }".into(),
        ..Default::default()
    }
    .into()];
    actions.extend((0..DOCS).map(add));
    let mut together = updates();
    together.extend([
        NewIndex {
            collection: "U".into(),
            index: index_on("by_g", "g"),
            expected_data: succeeds(),
            common: Common {
                envelope: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        NewIndex {
            collection: "U".into(),
            index: index_on("by_h", "h"),
            expected_data: succeeds(),
            common: Common {
                envelope: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]);
    actions.extend([
        Parallel {
            actions: together,
            ..Default::default()
        }
        .into(),
        ListIndexes {
            collection: "U".into(),
            expected_data: json!({
                "indexes": m::unordered(json!([
                    m::object(json!({
                        "Description": m::object(json!({
                            "Name": "by_g",
                            "Fields": [m::object(json!({ "Name": "g" }))],
                        })),
                        "Execution": m::object(json!({ "Status": 3 })),
                    })),
                    m::object(json!({
                        "Description": m::object(json!({
                            "Name": "by_h",
                            "Fields": [m::object(json!({ "Name": "h" }))],
                        })),
                        "Execution": m::object(json!({ "Status": 3 })),
                    })),
                ])),
            }),
            ..Default::default()
        }
        .into(),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.ddl_under_writes")]
fn activating_a_version_publishes_its_indexes() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type U { g: Int }".into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("U".into()),
                common: Common {
                    capture: captures([("first_version", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{"op":"add","path":"/U/Fields/-","value":{"Name":"h","Kind":11}}]"#
                    .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("U".into()),
                common: Common {
                    capture: captures([("second_version", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "U".into(),
                index: index_on("by_value", "g"),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: Text::reference("first_version"),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "U".into(),
                expected_data: json!({ "indexes": [] }),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: Text::reference("second_version"),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "U".into(),
                expected_data: json!({
                    "indexes": [m::object(json!({
                        "Description": m::object(json!({ "Name": "by_value" })),
                        "Execution": m::object(json!({ "Status": 3 })),
                    }))],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
