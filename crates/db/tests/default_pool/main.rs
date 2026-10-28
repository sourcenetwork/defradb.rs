//! A database given no pool runs its pool work on the process's default
//! pool. A binary of its own: the test registers that pool for the process.
#[path = "../common/mod.rs"]
mod common;

use std::sync::Arc;

use common::schema::test_schema;
use db::{AutoCommitMutator, DB};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use storage::RegolithStore;

const DOCUMENTS: usize = 64;

fn documents() -> Vec<Document> {
    (0..DOCUMENTS)
        .map(|i| {
            let mut doc = Document::new(test_schema());
            doc.set("name", NormalValue::String(format!("user-{i}")))
                .unwrap();
            doc
        })
        .collect()
}

async fn pool_less_db() -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(test_schema()).await.unwrap();
    db
}

fn tasks(pool: &workers::Pool) -> Vec<u64> {
    pool.stats()
        .workers
        .iter()
        .map(|worker| worker.tasks)
        .collect()
}

#[test]
fn create_many_without_a_pool_computes_blocks_across_the_default_pools_workers() {
    workers::block_on(async {
        let pool = workers::Pool::start(workers::Config::unplaced(4)).unwrap();
        assert!(pool.make_default());

        let batched = pool_less_db().await;
        assert!(batched.pool().is_none());
        let before = tasks(&pool);
        let created = AutoCommitMutator::new(batched)
            .create_many("Users", documents())
            .await
            .unwrap();
        let ran: Vec<u64> = tasks(&pool)
            .iter()
            .zip(&before)
            .map(|(after, before)| after - before)
            .collect();
        assert!(
            ran.iter().all(|tasks| *tasks > 0),
            "every worker of the default pool computes a run of the documents: {ran:?}"
        );

        // The same documents, one create at a time, on another database.
        let single = AutoCommitMutator::new(pool_less_db().await);
        assert_eq!(created.len(), DOCUMENTS);
        for (doc, batched) in documents().into_iter().zip(&created) {
            let one = single.create("Users", doc).await.unwrap();
            assert_eq!(one.doc_id, batched.doc_id);
            assert_eq!(one.commit_cid, batched.commit_cid);
            assert_eq!(one.document, batched.document);
        }
        pool.shutdown_blocking().unwrap();
    })
    .unwrap()
}
