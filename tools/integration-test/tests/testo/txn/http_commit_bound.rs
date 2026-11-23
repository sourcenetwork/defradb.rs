use integration_test::actions::{
    AddCollection, HoldCommit, NewNode, ReleaseCommit, Request, TxnBegin,
};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.txn.http_commit_bound")]
fn an_http_commit_past_its_bound_answers_unknown_and_lands() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::RustHttp, ClientType::Cli]),
        actions: vec![
            NewNode {
                commit_gate: true,
                ..Default::default()
            }
            .into(),
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
            TxnBegin::default().into(),
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                        add_Users(input: [{ name: "John", age: 27 }]) {
                            _docID
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "add_Users": [{ "_docID": integration_test::m::doc_id() }],
                }),
                ..Default::default()
            }
            .into(),
            HoldCommit {
                txn: 0,
                expected_error: "commit outcome unknown after 1 milliseconds".into(),
                ..Default::default()
            }
            .into(),
            ReleaseCommit::default().into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "age": 27, "name": "John" }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
