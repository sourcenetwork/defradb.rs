//! Requests on explicit transactions as the query runner and the registry
//! send them: one at a time on the worker that owns the transaction, in the
//! order they arrived, however many are sent at once; a commit refused while
//! a reader the transaction lent is still open; and transactions spread over
//! the workers of a pool.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use db::{
    AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB,
};
use document::Document;
use futures::future::join_all;
use query::error::TransactionError;
use query::txn::{txn_job, TransactionRegistry};
use query::{QueryExecutor, QueryRequest, QueryRunner};
use regolith::sync::SegQueue;
use storage::RegolithStore;

use crate::common::owned::in_txn;

type Runner =
    QueryRunner<LensedAutoCommitFetcher<RegolithStore>, DbTransactionRegistry<RegolithStore>>;

async fn users_db() -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    for schema in query::parse_sdl("type Users { name: String }").unwrap() {
        db.create_collection(schema).await.unwrap();
    }
    db
}

fn runner(
    db: &Arc<DB<RegolithStore>>,
    registry: Arc<DbTransactionRegistry<RegolithStore>>,
) -> Runner {
    QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(db)),
        DbCollectionProvider::new_arc(Arc::clone(db)),
        registry,
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(db))))
}

/// The committed names, sorted.
async fn committed_names(runner: &Runner) -> Vec<String> {
    let response = runner
        .execute(QueryRequest::new("{ Users { name } }"))
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let mut names: Vec<String> = response.data.unwrap()["Users"]
        .as_array()
        .unwrap()
        .iter()
        .map(|user| user["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    names
}

/// Mutations sent at once on one handle all succeed: each runs its statement
/// alone on the transaction's owner, after the one that arrived before it. A
/// request that overlapped another would find the transaction lent and its
/// statement refused.
#[test]
fn concurrent_mutations_on_one_transaction_all_succeed() {
    workers::block_on(async {
        let db = users_db().await;
        let runner = runner(&db, Arc::new(DbTransactionRegistry::new(Arc::clone(&db))));
        let handle = runner.begin_txn(false).await.unwrap();

        let writes = (0..8).map(|i| {
            runner.execute_in_txn(
                QueryRequest::new(format!(
                    r#"mutation {{ add_Users(input: {{name: "user-{i}"}}) {{ name }} }}"#
                )),
                &handle,
            )
        });
        for response in join_all(writes).await {
            assert!(!response.has_errors(), "{:?}", response.errors);
        }
        assert!(
            committed_names(&runner).await.is_empty(),
            "nothing is visible before the commit"
        );

        runner.commit_txn(&handle).await.unwrap();
        let mut expected: Vec<String> = (0..8).map(|i| format!("user-{i}")).collect();
        expected.sort();
        assert_eq!(committed_names(&runner).await, expected);
    })
    .unwrap()
}

/// Requests sent at once on one transaction never overlap, and run in the
/// order they were sent.
#[test]
fn requests_on_one_transaction_never_overlap_and_keep_their_order() {
    workers::block_on(async {
        let db = users_db().await;
        let registry = DbTransactionRegistry::new(db);
        let handle = registry.begin(true).await.unwrap();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let order = Arc::new(SegQueue::new());

        let requests = (0..8).map(|i| {
            let (in_flight, order) = (Arc::clone(&in_flight), Arc::clone(&order));
            registry.run(
                &handle,
                None,
                txn_job(move |_| {
                    Box::pin(async move {
                        let overlapped = in_flight.fetch_add(1, Ordering::SeqCst) > 0;
                        // Holds its turn across a suspension, as a storage read does.
                        workers::delay(Duration::from_millis(2))
                            .expect("a timer on the owner")
                            .await
                            .expect("the pause ends");
                        in_flight.fetch_sub(1, Ordering::SeqCst);
                        order.push(i);
                        overlapped
                    })
                }),
            )
        });
        for overlapped in join_all(requests).await {
            assert!(!overlapped.unwrap(), "two requests ran at once");
        }
        let ran: Vec<usize> = std::iter::from_fn(|| order.pop()).collect();
        assert_eq!(ran, (0..8).collect::<Vec<_>>());
        registry.rollback(&handle).await.unwrap();
    })
    .unwrap()
}

/// A reader a request opened and kept past its end still holds the
/// transaction: the commit is refused, nothing of the transaction is
/// committed, and, as a refused commit always did, the transaction ends.
#[test]
fn a_commit_while_a_reader_is_still_open_is_refused_and_writes_nothing() {
    workers::block_on(async {
    let db = users_db().await;
    let registry = DbTransactionRegistry::new(Arc::clone(&db));
    let handle = registry.begin(false).await.unwrap();

    let reader = in_txn(&registry, &handle, |ctx| {
        Box::pin(async move {
            let mutator = ctx.doc_mutator().expect("mutator");
            mutator
                .create(
                    "Users",
                    Document::from_json_str(r#"{"name": "kept-out"}"#).unwrap(),
                )
                .await
                .expect("create");
            ctx.doc_fetcher()
                .stream_all_with_deleted("Users", false)
                .await
                .expect("stream")
        })
    })
    .await;

    let refused = registry.commit(&handle).await;
    assert!(
        matches!(&refused, Err(TransactionError::Execution(message)) if message.contains("still holds the transaction")),
        "{refused:?}"
    );
    assert_eq!(registry.active_transaction_count().unwrap(), 0);
    assert!(matches!(
        registry.commit(&handle).await,
        Err(TransactionError::NotFound(_))
    ));
    drop(reader);

    let committed = registry
        .read(
            None,
            txn_job(|turn| {
                Box::pin(async move {
                    let ctx = turn.context().expect("a snapshot");
                    ctx.doc_fetcher().get_all("Users").await.expect("read")
                })
            }),
        )
        .await
        .unwrap();
    assert!(
        committed.is_empty(),
        "the refused commit wrote {committed:?}"
    );
})
    .unwrap()
}

/// Transactions on a pool of several workers, each written and committed at
/// once with the others: every one commits what it wrote.
#[test]
fn transactions_spread_over_a_pool_commit_side_by_side() {
    workers::block_on(async {
        let db = users_db().await;
        let pool = workers::Pool::start(workers::Config::unplaced(3)).unwrap();
        let registry = Arc::new(DbTransactionRegistry::with_pool(
            Arc::clone(&db),
            pool.clone(),
            None,
        ));
        let runner = runner(&db, Arc::clone(&registry));

        let handles: Vec<_> = join_all((0..6).map(|_| registry.begin(false)))
            .await
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let writes = handles.iter().enumerate().map(|(i, handle)| {
            runner.execute_in_txn(
                QueryRequest::new(format!(
                    r#"mutation {{ add_Users(input: {{name: "user-{i}"}}) {{ name }} }}"#
                )),
                handle,
            )
        });
        for response in join_all(writes).await {
            assert!(!response.has_errors(), "{:?}", response.errors);
        }
        for committed in join_all(handles.iter().map(|handle| registry.commit(handle))).await {
            committed.unwrap();
        }

        let mut expected: Vec<String> = (0..6).map(|i| format!("user-{i}")).collect();
        expected.sort();
        assert_eq!(committed_names(&runner).await, expected);
        assert_eq!(registry.active_transaction_count().unwrap(), 0);
        drop(runner);
        drop(registry);
        pool.stop().await.unwrap();
    })
    .unwrap()
}
