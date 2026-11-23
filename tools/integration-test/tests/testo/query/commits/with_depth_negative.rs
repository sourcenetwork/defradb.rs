//! `depth: -1` on `_commits` is unlimited and any other negative depth is
//! refused, and a walk from a merge commit lists its branches last head
//! first, as Go's DAG scan does (`go.query.commits.depth`).

use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, NewNode, Request,
    UpdateDoc, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;
use testo::action::ActionInstance;

fn doc_with_two_updates() -> Vec<ActionInstance<Actions>> {
    vec![
        AddCollection {
            sdl: r#"
                type Users {
                    name: String
                    age: Int
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "John", "age": 21 })),
            common: Common {
                capture: captures([("doc0_0", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        UpdateDoc {
            collection: "Users".into(),
            doc_id: Text::reference("doc0_0"),
            doc_map: json!({ "age": 22 }),
            expected_data: json!({}),
            ..Default::default()
        }
        .into(),
        UpdateDoc {
            collection: "Users".into(),
            doc_id: Text::reference("doc0_0"),
            doc_map: json!({ "age": 23 }),
            expected_data: json!({}),
            ..Default::default()
        }
        .into(),
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("doc0_0"),
                Text::from(r#"") { cid fieldName height } }"#),
            ]),
            expected_data: json!({ "_commits": m::length(7) }),
            common: Common {
                capture: captures([("all", "/data/_commits")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn same_as_no_depth(depth: &str) -> Vec<ActionInstance<Actions>> {
    let mut actions = doc_with_two_updates();
    actions.push(
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("doc0_0"),
                Text::from(format!(
                    r#"", depth: {depth}) {{ cid fieldName height }} }}"#
                )),
            ]),
            expected_data: json!({ "_commits": m::r("all") }),
            ..Default::default()
        }
        .into(),
    );
    actions
}

#[test]
#[mantra_macros::req_test("go.query.commits.depth")]
fn query_commits_with_depth_minus_one_returns_all_commits() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: same_as_no_depth("-1"),
        ..Default::default()
    }
    .execute();
}

/// Go casts every negative depth to a huge one; defradb.rs keeps only `-1`
/// as unlimited and refuses the rest, so the Go client is left out.
#[test]
#[mantra_macros::req_test("go.query.commits.depth")]
fn query_commits_with_depth_minus_two_returns_error() {
    let mut actions = doc_with_two_updates();
    actions.push(
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("doc0_0"),
                Text::from(r#"", depth: -2) { cid } }"#),
            ]),
            expected_error: Some("depth must be non-negative, or -1 for no limit".into()),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.depth")]
fn query_commits_with_doc_id_cid_and_depth_minus_one_returns_ancestors() {
    let mut actions = doc_with_two_updates();
    actions.push(
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("doc0_0"),
                Text::from(r#"", depth: 1, filter: {fieldName: {_eq: "_C"}}) { cid } }"#),
            ]),
            common: Common {
                capture: captures([("head", "/data/_commits/0/cid")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("doc0_0"),
                Text::from(r#"", cid: ""#),
                Text::reference("head"),
                Text::from(r#"", depth: -1) { cid fieldName height } }"#),
            ]),
            expected_data: json!({
                "_commits": [
                    { "cid": m::r("head"), "fieldName": "_C", "height": 3 },
                    { "cid": m::cid(), "fieldName": "_C", "height": 2 },
                    { "cid": m::cid(), "fieldName": "_C", "height": 1 },
                ],
            }),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn capture_on(node: &str, name: &str, path: &str) -> Common {
    Common {
        node: Some(node.into()),
        capture: captures([(name, path)]),
        ..Default::default()
    }
}

fn update(node: &str, name: &str) -> ActionInstance<Actions> {
    UpdateDoc {
        collection: "Users".into(),
        doc_id: Text::reference("doc0_0"),
        doc_map: json!({ "name": name }),
        expected_data: json!({}),
        common: Common {
            node: Some(node.into()),
            retry: true,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn head_on(node: &str, name: &str) -> ActionInstance<Actions> {
    WaitForSync {
        doc_ids: Some(vec![Text::reference("doc0_0")]),
        common: capture_on(node, name, "/cids/0"),
        ..Default::default()
    }
    .into()
}

fn heads_of(cid: &str, first: &str, second: Option<&str>) -> ActionInstance<Actions> {
    let mut capture = captures([(first, "/data/_commits/0/heads/0/cid")]);
    if let Some(second) = second {
        capture.insert(
            second.to_string(),
            "/data/_commits/0/heads/1/cid".to_string(),
        );
    }
    Request {
        request: Text::concat([
            Text::from(r#"query { _commits(docID: ""#),
            Text::reference("doc0_0"),
            Text::from(r#"", cid: ""#),
            Text::reference(cid),
            Text::from(r#"") { heads { cid } } }"#),
        ]),
        expected_data: json!({ "_commits": [{ "heads": m::length(if second.is_some() { 2 } else { 1 }) }] }),
        common: Common { node: Some("db0".into()), capture, ..Default::default() },
        ..Default::default()
    }
    .into()
}

/// One document whose history on db0 forks at its create `c0` into two
/// branches of two updates each, db0's and db1's, joined by db0's merge
/// commit `m`. `m`'s heads are captured as `mh0` and `mh1`, and the parent of
/// each as `p0` and `p1`.
pub(crate) fn merged_history() -> Vec<ActionInstance<Actions>> {
    let sdl = r#"
        type Users {
            name: String
        }
    "#;
    vec![
        NewNode {
            network: true,
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        NewNode {
            network: true,
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: sdl.into(),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: sdl.into(),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "John" })),
            common: Common {
                node: Some("db0".into()),
                retry: true,
                capture: captures([("doc0_0", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        head_on("db0", "c0"),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "John" })),
            common: Common {
                node: Some("db1".into()),
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        head_on("db1", "c0_db1"),
        update("db0", "Fred"),
        head_on("db0", "a1"),
        update("db0", "Freddy"),
        head_on("db0", "a2"),
        update("db1", "Shahzad"),
        head_on("db1", "b1"),
        ConnectPeers {
            target: "db0".into(),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        AddCollectionSubscription {
            collections: vec!["Users".into()],
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        AddCollectionSubscription {
            collections: vec!["Users".into()],
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        update("db1", "Chris"),
        head_on("db1", "b2"),
        WaitForSync {
            doc_id: Some(Text::reference("doc0_0")),
            cid: Some(Text::reference("b2")),
            expected_data: json!({}),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("doc0_0"),
                Text::from(r#"", depth: 1, filter: {fieldName: {_eq: "_C"}}) { cid } }"#),
            ]),
            expected_data: json!({
                "_commits": m::unordered(json!([{ "cid": m::r("a2") }, { "cid": m::r("b2") }])),
            }),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        update("db0", "Addo"),
        head_on("db0", "m"),
        heads_of("m", "mh0", Some("mh1")),
        heads_of("mh0", "p0", None),
        heads_of("mh1", "p1", None),
    ]
}

#[test]
#[mantra_macros::req_test("go.query.commits.depth")]
fn query_commits_with_cid_and_depth_minus_one_on_merged_history_lists_last_head_first() {
    let mut actions = merged_history();
    actions.push(
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("doc0_0"),
                Text::from(r#"", cid: ""#),
                Text::reference("m"),
                Text::from(r#"", depth: -1) { cid } }"#),
            ]),
            expected_data: json!({
                "_commits": [
                    { "cid": m::r("m") },
                    { "cid": m::r("mh1") },
                    { "cid": m::r("p1") },
                    { "cid": m::r("c0") },
                    { "cid": m::r("mh0") },
                    { "cid": m::r("p0") },
                ],
            }),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}
