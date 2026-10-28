//! A transaction leaving the registry must release its `Arc<DB<S>>` there.
//!
//! The map retires the reference it removes instead of dropping it, so a
//! retained context would hold the store, and its on-disk lock, until some
//! later reclamation.

use db::database::DB;
use db::txn::registry::DbTransactionRegistry;
use query::txn::TransactionRegistry;
use std::path::PathBuf;
use std::sync::Arc;
use storage::RegolithStore;

enum Finish {
    Commit,
    Discard,
    Abandon,
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("db-{name}-{}-{nanos}", std::process::id()))
}

async fn assert_store_released(name: &str, finish: Finish) {
    let dir = temp_dir(name);
    let db = Arc::new(DB::new(RegolithStore::open(&dir).unwrap()).unwrap());
    let registry = DbTransactionRegistry::new(db.clone());

    let handle = registry.begin(false).await.unwrap();
    match finish {
        Finish::Commit => registry.commit(&handle).await.unwrap(),
        Finish::Discard => registry.discard(&handle).await.unwrap(),
        Finish::Abandon => registry.abandon(&handle),
    }

    assert_eq!(
        Arc::strong_count(&db),
        2,
        "the registry still holds the database after the transaction left it"
    );
    assert_eq!(registry.active_transaction_count().unwrap(), 0);

    drop(registry);
    drop(db);
    RegolithStore::open(&dir).expect("the store must reopen once the registry released it");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn commit_releases_the_store() {
    workers::block_on(async {
        assert_store_released("txn-store-release-commit", Finish::Commit).await;
    })
    .unwrap()
}

#[test]
fn discard_releases_the_store() {
    workers::block_on(async {
        assert_store_released("txn-store-release-discard", Finish::Discard).await;
    })
    .unwrap()
}

#[test]
fn abandon_releases_the_store() {
    workers::block_on(async {
        assert_store_released("txn-store-release-abandon", Finish::Abandon).await;
    })
    .unwrap()
}
