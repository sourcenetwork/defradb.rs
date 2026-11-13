//! Listing every collection's encrypted indexes in a transaction sees that
//! transaction's own adds and deletes, and nobody else sees them until it
//! commits, through every client.

use integration_test::actions::{
    AddCollection, DeleteEncryptedIndex, ListAllEncryptedIndexes, NewEncryptedIndex, TxnBegin,
    TxnCommit,
};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.txn.indexes.list-all-encrypted")]
fn txn_list_all_encrypted_indexes_sees_its_own_changes_until_commit() {
    let committed = json!({
        "indexes": { "Book": [{ "FieldName": "title", "Type": "equality" }] },
    });
    let changed = json!({
        "indexes": { "User": [{ "FieldName": "name", "Type": "equality" }] },
    });
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }

                    type Book {
                        title: String @encryptedIndex
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
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
            DeleteEncryptedIndex {
                txn: Some(0),
                collection: "Book".into(),
                field_name: "title".into(),
                ..Default::default()
            }
            .into(),
            ListAllEncryptedIndexes {
                txn: Some(0),
                expected_data: changed.clone(),
                ..Default::default()
            }
            .into(),
            ListAllEncryptedIndexes {
                expected_data: committed,
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            ListAllEncryptedIndexes {
                expected_data: changed,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
