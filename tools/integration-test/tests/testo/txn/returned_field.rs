//! A field an update returns is read, not decided on (`query::update_parts`),
//! so a concurrent write of it commits beside the update, and both writes land
//! (`defra.txn.returned_field_not_decided`). Go validates every field it
//! read, so its commit conflicts; the scenario runs on the Rust clients only.

use integration_test::actions::{AddCollection, AddDoc, Request, TxnBegin, TxnCommit, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.txn.returned_field_not_decided")]
fn an_update_commits_beside_a_write_of_a_field_it_returned() {
    let actions = vec![
        AddCollection {
            sdl: "type Snap { status: String  content: String }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Snap".into(),
            doc_map: Some(json!({ "status": "streaming", "content": "init" })),
            common: Common {
                capture: captures([("snap", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        TxnBegin::default().into(),
        Request {
            txn: Some(0),
            request: r#"mutation {
                update_Snap(input: {status: "claimed"}) { content }
            }"#
            .into(),
            expected_data: json!({ "update_Snap": [{ "content": "init" }] }),
            ..Default::default()
        }
        .into(),
        // Another writer changes the field the transaction's update returned,
        // and commits first.
        UpdateDoc {
            collection: "Snap".into(),
            doc_id: Text::reference("snap"),
            doc_map: json!({ "content": "changed" }),
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Snap { status content } }".into(),
            expected_data: json!({ "Snap": [{ "status": "claimed", "content": "changed" }] }),
            ..Default::default()
        }
        .into(),
    ];
    TestCase {
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..TestCase::from_actions(actions)
    }
    .execute();
}
