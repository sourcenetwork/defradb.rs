//! Generated from the conformance scenarios of tests/integration/mutation/delete/with_ids_txn_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, TxnBegin, TxnDiscard};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.txn.isolation")]
fn mutation_deletion_with_i_ds_and_txn() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        points: Float
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 26, "points": 48.48, "verified": true }),
                ),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: Text::concat([
                    Text::from(
                        r#"mutation {
					delete_User(docID: [""#,
                    ),
                    Text::reference("doc0"),
                    Text::from(
                        r#""]) {
						_docID
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "delete_User": [{ "_docID": m::r("doc0") }] }),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: Text::concat([
                    Text::from(
                        r#"query {
					User(docID: [""#,
                    ),
                    Text::reference("doc0"),
                    Text::from(
                        r#""]) {
						_docID
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "User": [] }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
