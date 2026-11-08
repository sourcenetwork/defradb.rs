//! `_commits` filtered on `height`. A user is created at height 1, then its
//! `age` and its `name` are updated, so its composite commits sit at heights
//! 1, 2 and 3. Every query names the document: that sends a height bound to
//! the document's height index instead of a walk from its heads. Go returns `{"data": null}` for these filters, so the test runs
//! only against the Rust node.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

fn commits(filter: &str, expected: Value) -> Actions {
    Request {
        request: Text::concat([
            Text::from(r#"query { _commits(docID: [""#),
            Text::reference("user"),
            Text::from(format!(
                r#""], filter: {filter}) {{ fieldName height }} }}"#
            )),
        ]),
        expected_data: json!({ "_commits": expected }),
        common: Common {
            unordered_documents: vec!["/data/_commits".into()],
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn composite(heights: &[u64]) -> Value {
    heights
        .iter()
        .map(|height| json!({ "fieldName": "_C", "height": height }))
        .collect()
}

fn update(doc_map: Value) -> Actions {
    UpdateDoc {
        collection: "User".into(),
        doc_id: Text::reference("user"),
        doc_map,
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.commits.height_filter")]
fn commits_filter_on_height_ranges() {
    let actions: Vec<Actions> = vec![
        AddCollection {
            sdl: "type User { name: String  age: Int }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "Alice", "age": 30 })),
            common: Common {
                capture: captures([("user", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        update(json!({ "age": 31 })),
        update(json!({ "name": "Alicia" })),
        commits(r#"{fieldName: {_eq: "_C"}}"#, composite(&[1, 2, 3])),
        commits(
            r#"{height: {_gte: 2, _lte: 3}, fieldName: {_eq: "_C"}}"#,
            composite(&[2, 3]),
        ),
        commits(
            r#"{height: {_gte: 2, _lt: 5}, fieldName: {_eq: "_C"}}"#,
            composite(&[2, 3]),
        ),
        commits(
            r#"{height: {_gte: 2}, fieldName: {_eq: "_C"}}"#,
            composite(&[2, 3]),
        ),
        commits(
            r#"{height: {_lte: 2}, fieldName: {_eq: "_C"}}"#,
            composite(&[1, 2]),
        ),
        commits(
            r#"{height: {_gt: 1, _lt: 3}, fieldName: {_eq: "_C"}}"#,
            composite(&[2]),
        ),
        commits(
            r#"{height: {_eq: 2}, fieldName: {_eq: "age"}}"#,
            json!([{ "fieldName": "age", "height": 2 }]),
        ),
        commits(
            r#"{_and: [{height: {_gte: 2}}, {height: {_lt: 4}}, {fieldName: {_eq: "_C"}}]}"#,
            composite(&[2, 3]),
        ),
        commits(
            r#"{_or: [
                {_and: [{height: {_eq: 1}}, {fieldName: {_eq: "_C"}}]},
                {_and: [{height: {_eq: 3}}, {fieldName: {_eq: "_C"}}]}
            ]}"#,
            composite(&[1, 3]),
        ),
        commits(r#"{height: {_gt: 10}, fieldName: {_eq: "_C"}}"#, json!([])),
        commits(r#"{height: {_gt: 10, _lt: 5}}"#, json!([])),
    ];
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}
