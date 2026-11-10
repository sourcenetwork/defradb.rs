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
        let adapter = EncryptedIndexAdapter::new_arc(Arc::clone(&db), runner)
            .bind_request(&in_txn)
            .unwrap();

        adapter.add_encrypted_index("User", "name").await.unwrap();
        assert_eq!(encrypted_on_user(&db), 0, "not committed yet");
        let listed = adapter.list_encrypted_indexes(Some("User")).await.unwrap();
        assert_eq!(listed.len(), 1, "the transaction sees its own index");

        registry.commit(&handle).await.unwrap();
        assert_eq!(encrypted_on_user(&db), 1, "the commit lands it");
    })
    .unwrap();
}
