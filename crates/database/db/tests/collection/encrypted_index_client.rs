//! The core's encrypted index operations, which every client calls: one in
//! a transaction lands on its commit, one outside commits on its own, and a
//! missing collection or transaction is reported in Go's words.

use std::sync::Arc;

use db::{DbTransactionRegistry, EncryptedIndexError, DB};
use defra_core::{RequestContext, TxnId};
use query::txn::TransactionRegistry;
use storage::RegolithStore;

async fn registry_with(
    sdl: &str,
) -> (Arc<DB<RegolithStore>>, DbTransactionRegistry<RegolithStore>) {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.add_collection(&RequestContext::default(), sdl)
        .await
        .unwrap();
    let registry = DbTransactionRegistry::new(Arc::clone(&db));
    (db, registry)
}

fn encrypted_on_user(db: &DB<RegolithStore>) -> usize {
    db.get_collection("User")
        .unwrap()
        .unwrap()
        .schema()
        .encrypted_indexes
        .len()
}

#[test]
fn an_encrypted_index_added_in_a_transaction_lands_on_its_commit() {
    workers::block_on(async {
        let (db, registry) = registry_with("type User { name: String }").await;
        let outside = RequestContext::default();
        let handle = registry.begin_as(&outside, false).await.unwrap();
        let in_txn = outside
            .clone()
            .with_txn(Some(handle.as_str().parse::<TxnId>().unwrap()));

        registry
            .add_encrypted_index_as(&in_txn, "User", "name")
            .await
            .unwrap();
        assert_eq!(encrypted_on_user(&db), 0, "not committed yet");
        let listed = registry
            .encrypted_indexes_as(&in_txn, "User")
            .await
            .unwrap();
        assert_eq!(listed, ["name"], "the transaction sees its own index");
        let all = registry.all_encrypted_indexes_as(&in_txn).await.unwrap();
        assert_eq!(all, [("User".to_owned(), vec!["name".to_owned()])]);
        let committed = registry.all_encrypted_indexes_as(&outside).await.unwrap();
        assert!(
            committed.is_empty(),
            "nobody else sees it before the commit"
        );

        registry.commit(&handle).await.unwrap();
        assert_eq!(encrypted_on_user(&db), 1, "the commit lands it");
        let all = registry.all_encrypted_indexes_as(&outside).await.unwrap();
        assert_eq!(all.len(), 1, "everyone sees it once committed");
    })
    .unwrap();
}

#[test]
fn encrypted_index_changes_outside_a_transaction_keep_each_other() {
    workers::block_on(async {
        let (db, registry) = registry_with("type User { name: String age: Int }").await;
        let context = RequestContext::default();

        registry
            .add_encrypted_index_as(&context, "User", "name")
            .await
            .unwrap();
        registry
            .add_encrypted_index_as(&context, "User", "age")
            .await
            .unwrap();
        assert_eq!(encrypted_on_user(&db), 2, "the second add keeps the first");
        let again = registry
            .add_encrypted_index_as(&context, "User", "name")
            .await;
        assert_eq!(
            again.unwrap_err().to_string(),
            "encrypted index already exists on this field. Field: name"
        );

        registry
            .delete_encrypted_index_as(&context, "User", "name")
            .await
            .unwrap();
        let listed = registry
            .encrypted_indexes_as(&context, "User")
            .await
            .unwrap();
        assert_eq!(listed, ["age"]);
    })
    .unwrap();
}

#[test]
fn a_missing_collection_is_reported_alike_in_and_outside_a_transaction() {
    workers::block_on(async {
        let (_db, registry) = registry_with("type User { name: String }").await;
        let outside = RequestContext::default();
        let handle = registry.begin_as(&outside, false).await.unwrap();
        let in_txn = outside
            .clone()
            .with_txn(Some(handle.as_str().parse::<TxnId>().unwrap()));

        for context in [&outside, &in_txn] {
            let added = registry.add_encrypted_index_as(context, "Nope", "name").await;
            let listed = registry.encrypted_indexes_as(context, "Nope").await;
            let deleted = registry
                .delete_encrypted_index_as(context, "Nope", "name")
                .await;
            for error in [added.unwrap_err(), listed.unwrap_err(), deleted.unwrap_err()] {
                assert!(
                    matches!(&error, EncryptedIndexError::CollectionMissing(name) if name == "Nope"),
                    "{error:?}"
                );
                assert_eq!(error.to_string(), "collection not found. Name: Nope");
            }
        }
    })
    .unwrap();
}

#[test]
fn a_transaction_that_is_not_open_is_reported_in_gos_words() {
    workers::block_on(async {
        let (_db, registry) = registry_with("type User { name: String }").await;
        let outside = RequestContext::default();
        let handle = registry.begin_as(&outside, false).await.unwrap();
        let in_txn = outside
            .clone()
            .with_txn(Some(handle.as_str().parse::<TxnId>().unwrap()));
        registry.discard(&handle).await.unwrap();

        let added = registry
            .add_encrypted_index_as(&in_txn, "User", "name")
            .await;
        assert!(
            matches!(added, Err(EncryptedIndexError::TransactionNotFound)),
            "{added:?}"
        );
        let all = registry.all_encrypted_indexes_as(&in_txn).await;
        assert_eq!(all.unwrap_err().to_string(), "transaction not found");
    })
    .unwrap();
}
