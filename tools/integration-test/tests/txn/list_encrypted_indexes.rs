//! Generated from the conformance scenarios of tests/integration/txn/list_encrypted_indexes_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, ListEncryptedIndexes, NewEncryptedIndex, TxnBegin, TxnCommit,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.indexes.read-own-writes")]
fn txn_add_encrypted_index_exhibits_transactional_isolation_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddCollection {
                txn: Some(0),
                sdl: r#"
                    type User {
                        name: String 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                txn: Some(0),
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                ..Default::default()
            }
            .into(),
            NewEncryptedIndex {
                txn: Some(0),
                collection: "User".into(),
                field_name: "name".into(),
                index_type: "".into(),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([{ "FieldName": "name", "Type": "equality" }])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
