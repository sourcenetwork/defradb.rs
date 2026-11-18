use std::sync::Arc;

use db::{DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB};
use defra_core::{RequestContext, TxnId};
use query::txn::TransactionRegistry;
use query::QueryRunner;
use storage::RegolithStore;

use super::EncryptedIndexAdapter;

fn encrypted_on_user(db: &DB<RegolithStore>) -> usize {
    db.get_collection("User")
        .unwrap()
        .unwrap()
        .schema()
        .encrypted_indexes
        .len()
}

/// `defradb start` serves encrypted indexes through this adapter: one added
/// with the transaction header lands in that transaction, never on its own.
#[test]
fn an_encrypted_index_added_in_a_transaction_lands_on_its_commit() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let context = RequestContext::default();
        db.add_collection(&context, "type User { name: String }")
            .await
            .unwrap();
        let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&db)));
        let runner = Arc::new(QueryRunner::with_arc_registry_and_provider(
            LensedAutoCommitFetcher::new(Arc::clone(&db)),
            DbCollectionProvider::new_arc(Arc::clone(&db)),
            Arc::clone(&registry),
        ));
        let handle = registry.begin_as(&context, false).await.unwrap();
        let in_txn = context
            .clone()
            .with_txn(Some(handle.as_str().parse::<TxnId>().unwrap()));
        let outside = EncryptedIndexAdapter::new_arc(Arc::clone(&db), runner);
        let adapter = outside.bind_request(&in_txn).unwrap();

        adapter.add_encrypted_index("User", "name").await.unwrap();
        assert_eq!(encrypted_on_user(&db), 0, "not committed yet");
        let listed = adapter.list_encrypted_indexes(Some("User")).await.unwrap();
        assert_eq!(listed.len(), 1, "the transaction sees its own index");
        let all = adapter.list_encrypted_indexes(None).await.unwrap();
        assert_eq!(
            all.len(),
            1,
            "listing every index sees it in the transaction"
        );
        assert_eq!(
            (all[0].collection.as_str(), all[0].field_name.as_str()),
            ("User", "name")
        );
        let committed = outside.list_encrypted_indexes(None).await.unwrap();
        assert!(
            committed.is_empty(),
            "nobody else sees it before the commit"
        );

        registry.commit(&handle).await.unwrap();
        assert_eq!(encrypted_on_user(&db), 1, "the commit lands it");
        let all = outside.list_encrypted_indexes(None).await.unwrap();
        assert_eq!(all.len(), 1, "everyone sees it once committed");
    })
    .unwrap();
}

/// Without a transaction header each add and delete commits on its own and
/// keeps the indexes other requests committed before it.
#[test]
fn encrypted_index_changes_outside_a_transaction_keep_each_other() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let context = RequestContext::default();
        db.add_collection(&context, "type User { name: String age: Int }")
            .await
            .unwrap();
        let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&db)));
        let runner = Arc::new(QueryRunner::with_arc_registry_and_provider(
            LensedAutoCommitFetcher::new(Arc::clone(&db)),
            DbCollectionProvider::new_arc(Arc::clone(&db)),
            registry,
        ));
        let adapter = EncryptedIndexAdapter::new_arc(Arc::clone(&db), runner);

        adapter.add_encrypted_index("User", "name").await.unwrap();
        adapter.add_encrypted_index("User", "age").await.unwrap();
        assert_eq!(encrypted_on_user(&db), 2, "the second add keeps the first");
        assert!(adapter.add_encrypted_index("User", "name").await.is_err());

        adapter
            .delete_encrypted_index("User", "name")
            .await
            .unwrap();
        let listed = adapter.list_encrypted_indexes(Some("User")).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].field_name, "age");
    })
    .unwrap();
}
