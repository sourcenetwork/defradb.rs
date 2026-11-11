//! Generated from the conformance scenarios of tests/integration/mutation/update/txn_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, TxnBegin, TxnCommit, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.txn.conflict")]
fn mutation_update_interleaved_txn() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                txn: Some(0),
                doc_id: Text::reference("doc0"),
                doc_map: json!({ "name": "Johnny" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                txn: Some(1),
                doc_id: Text::reference("doc1"),
                doc_map: json!({ "name": "Freddy" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 1,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
