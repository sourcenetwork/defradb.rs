use db::database::DB;
use db::txn::registry::DbTransactionRegistry;
use defra_core::current_identity::with_scoped_identity;
use query::error::TransactionError;
use query::txn::{txn_job, TransactionRegistry};
use std::sync::Arc;
use std::time::Duration;
use storage::RegolithStore;

/// A request that only proves it reached the transaction.
async fn touch(
    registry: &DbTransactionRegistry<RegolithStore>,
    handle: &query::TransactionHandle,
) -> Result<(), TransactionError> {
    registry
        .run(handle, None, txn_job(|_| Box::pin(async {})))
        .await
}

#[test]
fn transaction_ownership_rejects_foreign_access_without_consuming_handle() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let registry = DbTransactionRegistry::new(db);
        for owner in [None, Some("did:key:alice".to_string())] {
            let handle = with_scoped_identity(owner.clone(), registry.begin(false))
                .await
                .unwrap();
            for caller in [None, Some("did:key:bob".to_string())] {
                if caller == owner {
                    continue;
                }
                with_scoped_identity(caller, async {
                    assert!(matches!(
                        touch(&registry, &handle).await,
                        Err(TransactionError::NotFound(_))
                    ));
                    assert!(matches!(
                        registry.commit(&handle).await,
                        Err(TransactionError::NotFound(_))
                    ));
                    assert!(matches!(
                        registry.discard(&handle).await,
                        Err(TransactionError::NotFound(_))
                    ));
                })
                .await;
                assert_eq!(registry.active_transaction_count().unwrap(), 1);
                with_scoped_identity(owner.clone(), touch(&registry, &handle))
                    .await
                    .expect("the owner still has its transaction");
            }
            with_scoped_identity(owner.clone(), registry.commit(&handle))
                .await
                .unwrap();
            assert!(matches!(
                with_scoped_identity(owner, touch(&registry, &handle)).await,
                Err(TransactionError::NotFound(_))
            ));
        }
        assert_eq!(registry.active_transaction_count().unwrap(), 0);
    })
    .unwrap()
}

#[test]
fn transaction_ownership_preserves_implicit_reads_and_idle_cleanup() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let registry = DbTransactionRegistry::new(db);
        with_scoped_identity(Some("did:key:alice".to_string()), async {
            let served = registry
                .read(
                    None,
                    txn_job(|turn| {
                        Box::pin(async move { turn.context().map(|ctx| ctx.is_readonly()) })
                    }),
                )
                .await
                .unwrap();
            assert!(
                served.unwrap(),
                "an implicit read is served from a snapshot"
            );
            assert_eq!(
                registry.active_transaction_count().unwrap(),
                0,
                "an implicit read is never registered"
            );
            registry.begin(false).await.unwrap();
        })
        .await;
        let result = registry
            .cleanup_stale_transactions(Duration::ZERO)
            .await
            .unwrap();
        assert!(result.is_complete());
        assert_eq!(result.cleaned, 1);
        assert_eq!(registry.active_transaction_count().unwrap(), 0);
    })
    .unwrap()
}
