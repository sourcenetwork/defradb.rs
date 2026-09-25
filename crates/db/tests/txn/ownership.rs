use db::database::DB;
use db::txn::registry::DbTransactionRegistry;
use defra_core::current_identity::with_scoped_identity;
use query::error::TransactionError;
use query::txn::{GetTransactionResult, TransactionRegistry};
use std::sync::Arc;
use std::time::Duration;
use storage::RegolithStore;

#[tokio::test]
async fn transaction_ownership_rejects_foreign_access_without_consuming_handle() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let registry = DbTransactionRegistry::new(db);
    for owner in [None, Some("did:key:alice".to_string())] {
        let handle = with_scoped_identity(owner.clone(), registry.begin(false))
            .await
            .unwrap();
        let ctx = with_scoped_identity(owner.clone(), async {
            registry.get_ctx(handle.as_str()).unwrap().unwrap()
        })
        .await;
        let last_seen = ctx.last_request_seen();
        for caller in [None, Some("did:key:bob".to_string())] {
            if caller == owner {
                continue;
            }
            with_scoped_identity(caller, async {
                assert!(matches!(
                    registry.get(&handle),
                    GetTransactionResult::NotFound
                ));
                assert!(registry.get_ctx(handle.as_str()).unwrap().is_none());
                assert!(matches!(
                    registry.commit(&handle).await,
                    Err(TransactionError::NotFound(_))
                ));
                assert!(matches!(
                    registry.rollback(&handle).await,
                    Err(TransactionError::NotFound(_))
                ));
                assert!(matches!(
                    registry.finish_implicit_read(&handle, true).await,
                    Err(TransactionError::NotFound(_))
                ));
            })
            .await;
            assert_eq!(ctx.last_request_seen(), last_seen);
            assert!(!ctx.is_consumed().await);
        }
        with_scoped_identity(owner, registry.commit(&handle))
            .await
            .unwrap();
        assert!(ctx.is_consumed().await);
    }
    assert_eq!(registry.active_transaction_count().unwrap(), 0);
}

#[tokio::test]
async fn transaction_ownership_preserves_implicit_reads_and_idle_cleanup() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let registry = DbTransactionRegistry::new(db);
    with_scoped_identity(Some("did:key:alice".to_string()), async {
        let handle = registry.begin_implicit_read().await.unwrap();
        assert!(matches!(
            registry.get(&handle),
            GetTransactionResult::Found(_)
        ));
        registry.finish_implicit_read(&handle, true).await.unwrap();
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
}
