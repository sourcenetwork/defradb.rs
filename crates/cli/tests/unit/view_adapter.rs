use std::sync::Arc;

use db::{DbTransactionRegistry, DB};
use defra_core::{RequestContext, TxnId};
use defra_http::router::ViewOperations;
use query::txn::TransactionRegistry;
use storage::RegolithStore;

use super::ViewAdapter;

/// `defradb start` serves views through this adapter: a view added with the
/// transaction header, over a type the same transaction created, lands in
/// that transaction, never on its own.
#[test]
fn a_view_added_in_a_transaction_lands_on_its_commit() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&db)));
        let context = RequestContext::default();
        let handle = registry.begin_as(&context, false).await.unwrap();
        registry
            .add_collection_in_txn_as(&context, handle.as_str(), "type User { name: String }")
            .await
            .unwrap();
        let in_txn = context
            .clone()
            .with_txn(Some(handle.as_str().parse::<TxnId>().unwrap()));
        let adapter = ViewAdapter::new_arc(
            Arc::clone(&db),
            Arc::clone(&registry),
            query::QueryLimits::default(),
        )
        .bind_request(&in_txn)
        .unwrap();

        adapter
            .add_view(
                "User { name }",
                "type UserView @materialized(if: false) { name: String }",
                None,
            )
            .await
            .unwrap();
        assert!(
            db.get_collection("UserView").unwrap().is_none(),
            "not committed yet"
        );

        registry.commit(&handle).await.unwrap();
        assert!(
            db.get_collection("UserView").unwrap().is_some(),
            "the commit lands it"
        );
    })
    .unwrap();
}
