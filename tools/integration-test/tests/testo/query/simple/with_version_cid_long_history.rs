//! A document queried by `cid` lists every version of its history in
//! `_version`, however long the history is (`go.query.simple.cid.doc-version`).

use integration_test::actions::{Actions, AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;
use testo::action::ActionInstance;

/// More updates than the 1000-commit walk `_version` once stopped at.
const UPDATES: i64 = 1001;

/// One process per update makes the CLI client too slow for this history,
/// and Go's cut-off is not what this checks, so both are left out.
#[test]
#[mantra_macros::req_test("go.query.simple.cid.doc-version")]
fn query_simple_with_version_cid_lists_a_history_over_a_thousand_versions() {
    let mut actions: Vec<ActionInstance<Actions>> = vec![
        AddCollection {
            sdl: r#"
                type Users {
                    age: Int
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "age": 0 })),
            common: Common {
                capture: captures([("doc0_0", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ];
    actions.extend((1..=UPDATES).map(|age| {
        UpdateDoc {
            collection: "Users".into(),
            doc_id: Text::reference("doc0_0"),
            doc_map: json!({ "age": age }),
            expected_data: json!({}),
            ..Default::default()
        }
        .into()
    }));
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
                Text::from(r#"query { Users(cid: ""#),
                Text::reference("head"),
                Text::from(r#"") { age _version { cid } } }"#),
            ]),
            expected_data: json!({
                "Users": [{ "age": UPDATES, "_version": m::length(UPDATES as u64 + 1) }],
            }),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp]),
        ..Default::default()
    }
    .execute();
}
