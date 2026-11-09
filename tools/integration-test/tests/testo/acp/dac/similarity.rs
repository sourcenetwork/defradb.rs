//! A similarity query never scores a document the reader may not see, at the
//! root or in a nested select (`defra.similarity.access_first`).

use integration_test::actions::{AddCollection, AddDACPolicy, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::USER_ACP_POLICY;
use serde_json::{json, Value};

const ALICE: u64 = 1;
const BOB: u64 = 2;
/// A query vector of two dimensions, against notes of three.
const SHORT: &str = "[1, 0]";
const WRONG_LENGTH: &str = "source and vector must be of the same length. Source: 3, Vector: 2";

fn as_identity(identity: u64) -> Common {
    Common {
        identity_seed: Some(identity),
        ..Default::default()
    }
}

/// `Note` under the local policy, written by an `Author`, and Alice's note about rivers.
fn setup() -> Vec<integration_test::actions::Actions> {
    vec![
        AddDACPolicy {
            policy: USER_ACP_POLICY.into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                identity_seed: Some(ALICE),
                capture: captures([("policy", "/policyID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: Text::concat([
                Text::from(r#"type Note @policy(id: ""#),
                Text::reference("policy"),
                Text::from(
                    r#"", resource: "users") {
                        title: String
                        vec: [Float32!]
                        author: Author
                    }
                    type Author {
                        name: String
                        notes: [Note]
                    }"#,
                ),
            ]),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Author".into(),
            doc_map: Some(json!({ "name": "alice" })),
            common: Common {
                capture: captures([("alice", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Note".into(),
            doc_map: Some(
                json!({ "title": "rivers", "vec": [1.0, 0.0, 0.0], "author": m::r("alice") }),
            ),
            common: as_identity(ALICE),
            ..Default::default()
        }
        .into(),
    ]
}

fn request(query: &str, identity: u64, expected: Value, error: Option<&str>) -> Request {
    Request {
        request: query.into(),
        expected_data: expected,
        expected_error: error.map(str::to_string),
        common: as_identity(identity),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.similarity.access_first")]
fn a_root_similarity_never_scores_a_hidden_note() {
    let query = format!("query {{ Note {{ title sim: SIMILARITY(vec: {{vector: {SHORT}}}) }} }}");
    let mut actions = setup();
    actions.push(request(&query, BOB, json!({ "Note": [] }), None).into());
    actions.push(request(&query, ALICE, Value::Null, Some(WRONG_LENGTH)).into());
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.similarity.access_first")]
fn a_nested_similarity_never_scores_a_hidden_note() {
    let query = format!(
        "query {{ Author {{ name notes {{ title sim: SIMILARITY(vec: {{vector: {SHORT}}}) }} }} }}"
    );
    let mut actions = setup();
    actions.push(
        request(
            &query,
            BOB,
            json!({ "Author": [{ "name": "alice", "notes": [] }] }),
            None,
        )
        .into(),
    );
    actions.push(request(&query, ALICE, Value::Null, Some(WRONG_LENGTH)).into());
    TestCase::from_actions(actions).execute();
}
