//! Every statement of a transaction resolves collections through that
//! transaction, so a collection it added is visible to its own queries,
//! mutations and introspection, and to nothing outside it.

use std::sync::Arc;

use db::{DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB};
use query::txn::TransactionRegistry;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use serde_json::json;
use storage::RegolithStore;

#[test]
fn statements_in_a_transaction_see_the_collection_it_added() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&db)));
        let handle = registry
            .begin_as(&defra_core::RequestContext::default(), false)
            .await
            .unwrap();
        registry
            .add_collection_in_txn_as(
                &defra_core::RequestContext::default(),
                handle.as_str(),
                "type TxnOnly { value: Int }",
            )
            .await
            .unwrap();
        let runner = QueryRunner::with_arc_registry_and_provider(
            LensedAutoCommitFetcher::new(Arc::clone(&db)),
            DbCollectionProvider::new_arc(Arc::clone(&db)),
            Arc::clone(&registry),
        );
        let in_txn = |query: &'static str| runner.execute_in_txn(QueryRequest::new(query), &handle);

        let created = in_txn("mutation { add_TxnOnly(input: {value: 7}) { value } }").await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let read = in_txn("{ TxnOnly { value } }").await;
        assert_eq!(read.data, Some(json!({ "TxnOnly": [{ "value": 7 }] })));
        let introspected = in_txn(r#"{ __type(name: "TxnOnly") { name } }"#).await;
        assert_eq!(
            introspected.data,
            Some(json!({ "__type": { "name": "TxnOnly" } }))
        );

        let outside = runner
            .execute(QueryRequest::new("{ TxnOnly { value } }"))
            .await;
        assert!(
            outside.has_errors(),
            "a collection added in an open transaction is visible outside it"
        );

        registry
            .rollback_as(&defra_core::RequestContext::default(), &handle)
            .await
            .unwrap();
    })
    .unwrap()
}
