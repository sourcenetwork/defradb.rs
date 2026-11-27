use integration_test::actions::{
    AddCollection, DeleteCollection, DeleteEncryptedIndex, ListEncryptedIndexes, NewEncryptedIndex,
    TxnBegin, TxnDiscard,
};
use integration_test::test_case::TestCase;

/// Go's text; the Rust node adds `. Name: Nope`.
const MISSING: &str = "collection not found";

fn new_index(txn: Option<usize>) -> NewEncryptedIndex {
    NewEncryptedIndex {
        txn,
        collection: "Nope".into(),
        field_name: "name".into(),
        index_type: "".into(),
        expected_error: Some(MISSING.into()),
        ..Default::default()
    }
}

fn list(txn: Option<usize>) -> ListEncryptedIndexes {
    ListEncryptedIndexes {
        txn,
        collection: "Nope".into(),
        expected_error: Some(MISSING.into()),
        ..Default::default()
    }
}

fn delete(txn: Option<usize>) -> DeleteEncryptedIndex {
    DeleteEncryptedIndex {
        txn,
        collection: "Nope".into(),
        field_name: "name".into(),
        expected_error: Some(MISSING.into()),
        ..Default::default()
    }
}

/// An encrypted index change or listing on a collection the node does not
/// hold is refused in Go's words, with no transaction and inside one alike.
#[test]
#[mantra_macros::req_test("defra.searchable_encryption.index.one_answer")]
fn encrypted_index_on_a_missing_collection_is_refused_in_and_outside_a_transaction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            new_index(None).into(),
            list(None).into(),
            delete(None).into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            new_index(Some(0)).into(),
            list(Some(0)).into(),
            delete(Some(0)).into(),
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

/// A transaction opened before a collection is deleted keeps its snapshot for
/// documents, but encrypted-index reads do not expose the deleted collection.
#[test]
#[mantra_macros::req_test("defra.searchable_encryption.index.one_answer")]
fn a_transaction_cannot_list_encrypted_indexes_for_a_deleted_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String @encryptedIndex }".into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            DeleteCollection {
                names: vec!["User".into()],
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                txn: Some(0),
                collection: "User".into(),
                expected_error: Some("collection not found. Name: User".into()),
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
