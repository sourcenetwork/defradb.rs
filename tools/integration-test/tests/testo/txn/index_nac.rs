//! Index operations in a transaction are gated by node access control as
//! they are outside one: `get-collection` first, as Go's collection lookup
//! checks it, through every client.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{
    AddCollection, Close, DeleteIndex, ListIndexes, NewIndex, Start, TxnBegin,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;

const NOT_AUTHORIZED: &str = "not authorized to perform operation. Permission: get-collection";

fn as_identity(seed: u64) -> Common {
    Common {
        identity_seed: Some(seed),
        ..Default::default()
    }
}

fn name_index() -> IndexSpec {
    IndexSpec {
        name: "User_name_ASC".into(),
        fields: vec![IndexField {
            name: "name".into(),
            descending: false,
        }],
        unique: false,
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.indexes")]
fn txn_index_operations_wrong_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: as_identity(1),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                common: as_identity(1),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: name_index(),
                common: as_identity(1),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                common: as_identity(2),
                ..Default::default()
            }
            .into(),
            NewIndex {
                txn: Some(0),
                collection: "User".into(),
                index: IndexSpec {
                    name: "User_age_ASC".into(),
                    fields: vec![IndexField {
                        name: "age".into(),
                        descending: false,
                    }],
                    unique: false,
                    ..Default::default()
                },
                expected_error: Some(NOT_AUTHORIZED.into()),
                common: as_identity(2),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                txn: Some(0),
                collection: "User".into(),
                expected_error: Some(NOT_AUTHORIZED.into()),
                common: as_identity(2),
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                txn: Some(0),
                collection: "User".into(),
                index_name: "User_name_ASC".into(),
                expected_error: Some(NOT_AUTHORIZED.into()),
                common: as_identity(2),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
