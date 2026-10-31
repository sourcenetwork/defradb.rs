//! A transaction belongs to the identity acting in the context that began it,
//! and only a request whose context names that identity reaches it again,
//! unless the registry is an embedder's, which serves every request.

use db::database::DB;
use db::txn::registry::{DbTransactionRegistry, TxnOwnership};
use db::{AutoCommitMutator, DbCollectionProvider, LensedAutoCommitFetcher};
use defra_core::RequestContext;
use identity::Did;
use query::error::TransactionError;
use query::txn::{txn_job, TransactionHandle, TransactionRegistry};
use query::{QueryExecutor, QueryRequest, QueryRunner, RestOperations, RestOperationsImpl};
use std::sync::Arc;
use storage::RegolithStore;

fn acting(did: Option<&str>) -> RequestContext {
    RequestContext::default().with_identity(did.map(str::to_owned))
}

/// Whether a request in `context` reaches transaction `handle`.
async fn reaches(
    registry: &DbTransactionRegistry<RegolithStore>,
    context: &RequestContext,
    handle: &TransactionHandle,
) -> bool {
    let job = txn_job(|turn| Box::pin(async move { turn.context().is_ok() }));
    match registry.run_as(context, handle, None, job).await {
        Ok(lent) => lent,
        Err(TransactionError::NotFound(_)) => false,
        Err(error) => panic!("unexpected registry error: {error}"),
    }
}

#[test]
fn only_the_context_that_began_a_transaction_reaches_it() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let registry = DbTransactionRegistry::new(db);
        let alice = acting(Some("did:key:alice"));
        let handle = registry.begin_as(&alice, false).await.unwrap();

        for stranger in [acting(None), acting(Some("did:key:bob"))] {
            assert!(!reaches(&registry, &stranger, &handle).await);
            assert!(registry
                .get_collections_in_txn_as(&stranger, handle.as_str())
                .await
                .is_err());
            assert!(matches!(
                registry.commit_as(&stranger, &handle).await,
                Err(TransactionError::NotFound(_))
            ));
        }

        // The owner set the old way, on this thread, reaches nothing either.
        defra_core::current_identity::set_current_identity(Some("did:key:alice".to_string()));
        let anonymous = reaches(&registry, &acting(None), &handle).await;
        defra_core::current_identity::set_current_identity(None);
        assert!(!anonymous);

        assert!(reaches(&registry, &alice, &handle).await);
        registry.commit_as(&alice, &handle).await.unwrap();
        assert_eq!(registry.active_transaction_count().unwrap(), 0);
    })
    .unwrap()
}

/// An embedder registry serves every request, whatever identity it acts as
/// and runs each request's work as that request's own identity.
#[test]
fn an_embedder_transaction_serves_every_identity_as_itself() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let registry = DbTransactionRegistry::new(db).with_ownership(TxnOwnership::Embedder);
        let handle = registry.begin_as(&acting(None), false).await.unwrap();

        for did in [None, Some("did:key:alice"), Some("did:key:bob")] {
            let caller = acting(did);
            let job = txn_job(|_| {
                Box::pin(async { defra_core::current_identity::get_effective_identity() })
            });
            let acted_as = registry.run_as(&caller, &handle, None, job).await.unwrap();
            assert_eq!(acted_as.as_deref(), did);
            assert!(registry
                .get_collections_in_txn_as(&caller, handle.as_str())
                .await
                .is_ok());
        }

        registry
            .commit_as(&acting(Some("did:key:bob")), &handle)
            .await
            .unwrap();
        assert_eq!(registry.active_transaction_count().unwrap(), 0);
    })
    .unwrap()
}

/// REST operations serve the request they are bound to: in a transaction that
/// request began they read and write it, and bound to any other caller they
/// find no transaction.
#[test]
fn rest_operations_reach_the_transaction_of_the_request_they_serve() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        for schema in query::parse_sdl("type Users { name: String }").unwrap() {
            db.create_collection(schema).await.unwrap();
        }
        let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&db)));
        let runner = Arc::new(
            QueryRunner::with_arc_registry_and_provider(
                LensedAutoCommitFetcher::new(Arc::clone(&db)),
                DbCollectionProvider::new_arc(Arc::clone(&db)),
                Arc::clone(&registry),
            )
            .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&db)))),
        );
        let rest = RestOperationsImpl::new(Arc::clone(&runner));
        let alice_did = Did::new("did:key:alice").unwrap();
        let alice = acting(Some(alice_did.as_str()));
        let handle = registry.begin_as(&alice, false).await.unwrap();

        for stranger in [acting(None), acting(Some("did:key:bob"))] {
            let foreign = rest
                .bind_request(&stranger)
                .unwrap()
                .with_transaction(handle.clone())
                .unwrap();
            assert!(foreign.list_collections().await.is_err());
            assert!(foreign
                .add_document("Users", serde_json::json!({ "name": "stranger" }), None)
                .await
                .is_err());
        }

        let owner = rest
            .bind_request(&alice)
            .unwrap()
            .with_transaction(handle.clone())
            .unwrap();
        assert_eq!(owner.list_collections().await.unwrap(), vec!["Users"]);
        owner
            .add_document(
                "Users",
                serde_json::json!({ "name": "alice" }),
                Some(&alice_did),
            )
            .await
            .unwrap();
        registry.commit_as(&alice, &handle).await.unwrap();

        let read = runner
            .execute(QueryRequest::new("{ Users { name } }"))
            .await;
        assert!(read.errors.is_empty(), "{:?}", read.errors);
        assert_eq!(
            read.data.unwrap()["Users"],
            serde_json::json!([{ "name": "alice" }])
        );
    })
    .unwrap()
}
