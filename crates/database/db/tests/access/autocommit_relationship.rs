use super::*;

#[test]
fn dropping_an_autocommit_transaction_before_commit_abandons_it() {
    workers::block_on(async {
        let database = std::sync::Arc::new(
            crate::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap(),
        );
        let registry = std::sync::Arc::new(crate::DbTransactionRegistry::new(database));
        let context = RequestContext::default();
        let handle = registry.begin_as(&context, false).await.unwrap();
        assert_eq!(registry.active_transaction_count().unwrap(), 1);
        {
            let _transaction = AutocommitTxn::new(registry.as_ref(), handle);
        }
        assert_eq!(registry.active_transaction_count().unwrap(), 0);
    })
    .unwrap();
}
