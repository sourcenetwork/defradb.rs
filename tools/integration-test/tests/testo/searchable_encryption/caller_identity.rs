//! Encrypted index checks use the request identity, never the signing node's
//! transaction owner identity.

use integration_test::actions::{
    AddCollection, Close, DeleteEncryptedIndex, ListAllEncryptedIndexes, ListEncryptedIndexes,
    NewEncryptedIndex, Start, TxnBegin, TxnDiscard,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;

const REFUSED: &str = "not authorized to perform operation. Permission: get-collection";

fn refused(txn: Option<usize>) -> NewEncryptedIndex {
    NewEncryptedIndex {
        txn,
        collection: "Users".into(),
        field_name: "name".into(),
        index_type: String::new(),
        expected_error: Some(REFUSED.into()),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.searchable_encryption.index.one_answer")]
fn an_anonymous_caller_cannot_use_the_signing_nodes_identity_in_a_transaction() {
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
                enable_signing: Some(true),
                node_identity_seed: Some(1),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String }".into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            refused(None).into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            refused(Some(0)).into(),
            ListEncryptedIndexes {
                txn: Some(0),
                collection: "Users".into(),
                expected_error: Some(REFUSED.into()),
                ..Default::default()
            }
            .into(),
            ListAllEncryptedIndexes {
                txn: Some(0),
                expected_error: Some(
                    "not authorized to perform operation. Permission: list-all-encrypted-index"
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
            DeleteEncryptedIndex {
                txn: Some(0),
                collection: "Users".into(),
                field_name: "name".into(),
                expected_error: Some(REFUSED.into()),
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
