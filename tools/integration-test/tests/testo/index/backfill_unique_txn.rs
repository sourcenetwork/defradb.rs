//! A unique index created in a transaction over duplicate values: as outside
//! a transaction, the create succeeds, and once the transaction commits the
//! index is listed with its failed backfill.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{
    AddCollection, AddDoc, ListIndexes, NewIndex, TxnBegin, TxnCommit,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

fn index_on(field: &str, unique: bool) -> IndexSpec {
    IndexSpec {
        name: "".into(),
        unique,
        fields: vec![IndexField {
            name: field.into(),
            descending: false,
        }],
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("go.index.unique.backfill")]
fn unique_index_in_txn_with_duplicate_values_fails_its_backfill_after_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age:  Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 21 })),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            NewIndex {
                txn: Some(0),
                collection: "User".into(),
                index: index_on("age", true),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            // Outside a transaction NewIndex waits until no index of the
            // collection is building, so the age index's backfill has ended
            // on every client, Go's included, by the listing below.
            NewIndex {
                collection: "User".into(),
                index: index_on("name", false),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            ListIndexes {
                txn: Some(1),
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({ "Name": "User_age_ASC", "Unique": true })),
                            "Execution": m::object(json!({
                                "Status": 2,
                                "Action": 3,
                                "Reason": m::substring("can not index"),
                            })),
                        })),
                        m::object(json!({
                            "Description": m::object(json!({ "Name": "User_name_ASC", "Unique": false })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
