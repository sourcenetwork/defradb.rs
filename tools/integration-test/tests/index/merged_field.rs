//! An index on a field merged by value follows the merged value
//! (`defra.index.merged_field`).

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, NewNode, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const NODES: [&str; 2] = ["db0", "db1"];

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// Two nodes, each with the collection and the same document, not yet
/// replicating, so their edits are concurrent.
fn apart(sdl: &str, doc: Value) -> Vec<Actions> {
    let mut actions = Vec::new();
    for node in NODES {
        actions.push(
            NewNode {
                network: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in NODES {
        actions.push(
            AddCollection {
                sdl: sdl.into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddDoc {
                collection: "Note".into(),
                doc_map: Some(doc.clone()),
                // The same content gives the same DocID on both nodes.
                common: Common {
                    capture: captures([(if node == "db0" { "doc" } else { "doc1" }, "/docID")]),
                    ..on(node)
                },
                ..Default::default()
            }
            .into(),
        );
    }
    actions
}

fn update(node: &str, doc: Value) -> Actions {
    UpdateDoc {
        collection: "Note".into(),
        doc_id: Text::reference("doc"),
        doc_map: doc,
        expected_data: json!({}),
        common: on(node),
        ..Default::default()
    }
    .into()
}

fn rejoin() -> Vec<Actions> {
    [("db0", "db1"), ("db1", "db0")]
        .into_iter()
        .map(|(from, to)| {
            AddReplicator {
                target: to.into(),
                common: on(from),
                ..Default::default()
            }
            .into()
        })
        .collect()
}

/// Waits until `filter` finds the document through the index on `node`.
fn merged(node: &str, filter: &str) -> Actions {
    Request {
        request: format!("query {{ Note(filter: {filter}) {{ _docID }} }}").into(),
        expected_data: json!({ "Note": [{ "_docID": m::r("doc") }] }),
        common: Common {
            eventually: Some(Eventually::default()),
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

/// What `filter` reads on `node`: `found` index entries, each a document.
fn fetches(node: &str, filter: &str, found: u64) -> Actions {
    Request {
        request: format!("query @explain(type: execute) {{ Note(filter: {filter}) {{ _docID }} }}")
            .into(),
        expected_data: json!(m::explain_scan(
            json!({ "indexFetches": found, "docFetches": found })
        )),
        common: on(node),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.index.merged_field")]
fn a_text_index_follows_local_edits() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Note { body: String @crdt(type: text) @index }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Note".into(),
            doc_map: Some(json!({ "body": "hello world" })),
            common: Common {
                capture: captures([("doc", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        UpdateDoc {
            collection: "Note".into(),
            doc_id: Text::reference("doc"),
            doc_map: json!({ "body": "hello brave world" }),
            expected_data: json!({}),
            ..Default::default()
        }
        .into(),
        fetches("db", r#"{body: {_eq: "hello brave world"}}"#, 1),
        fetches("db", r#"{body: {_eq: "hello world"}}"#, 0),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.merged_field")]
fn a_text_index_follows_merged_edits() {
    let mut actions = apart(
        "type Note { body: String @crdt(type: text) @index }",
        json!({ "body": "hello world" }),
    );
    actions.push(update("db0", json!({ "body": "hello brave world" })));
    actions.push(update("db1", json!({ "body": "hello world!" })));
    actions.extend(rejoin());
    for node in NODES {
        actions.push(merged(node, r#"{body: {_eq: "hello brave world!"}}"#));
        actions.push(fetches(node, r#"{body: {_eq: "hello brave world!"}}"#, 1));
        for stale in ["hello world", "hello brave world", "hello world!"] {
            actions.push(fetches(
                node,
                &format!(r#"{{body: {{_eq: "{stale}"}}}}"#),
                0,
            ));
        }
    }
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.merged_field")]
fn a_json_index_follows_merged_edits() {
    let mut actions = apart(
        "type Note { meta: JSON @crdt(type: json) @index }",
        json!({ "meta": { "a": 1, "b": 1 } }),
    );
    actions.push(update("db0", json!({ "meta": { "a": 2, "b": 1 } })));
    actions.push(update("db1", json!({ "meta": { "a": 1, "b": 2 } })));
    actions.extend(rejoin());
    for node in NODES {
        actions.push(merged(node, "{meta: {a: {_eq: 2}, b: {_eq: 2}}}"));
        for path in ["a", "b"] {
            actions.push(fetches(
                node,
                &format!("{{meta: {{{path}: {{_eq: 2}}}}}}"),
                1,
            ));
            actions.push(fetches(
                node,
                &format!("{{meta: {{{path}: {{_eq: 1}}}}}}"),
                0,
            ));
        }
    }
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.counter")]
fn a_counter_index_follows_local_increments() {
    let actions: Vec<Actions> = vec![
        AddCollection {
            sdl: "type Note { count: Int @crdt(type: pcounter) @index }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Note".into(),
            doc_map: Some(json!({ "count": 1 })),
            common: Common {
                capture: captures([("doc", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        update("db", json!({ "count": 2 })),
        update("db", json!({ "count": 3 })),
        fetches("db", "{count: {_eq: 6}}", 1),
        fetches("db", "{count: {_gt: 4}}", 1),
        fetches("db", "{count: {_eq: 1}}", 0),
        fetches("db", "{count: {_eq: 3}}", 0),
    ];
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.counter")]
fn a_counter_index_follows_merged_increments() {
    let mut actions = apart(
        "type Note { count: Int @crdt(type: pcounter) @index }",
        json!({ "count": 1 }),
    );
    actions.push(update("db0", json!({ "count": 2 })));
    actions.push(update("db1", json!({ "count": 3 })));
    actions.extend(rejoin());
    for node in NODES {
        actions.push(merged(node, "{count: {_eq: 6}}"));
        actions.push(fetches(node, "{count: {_eq: 6}}", 1));
        for stale in [1, 3, 4] {
            actions.push(fetches(node, &format!("{{count: {{_eq: {stale}}}}}"), 0));
        }
    }
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        without_db: true,
        ..Default::default()
    }
    .execute();
}
