#[path = "support/storage_full_env.rs"]
mod storage_full_env;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use storage::backends::regolith::{RegolithStore, RegolithStoreOptions};
use storage::corekv::{Error, Store};
use storage_full_env::FullEnv;

async fn write(store: &RegolithStore, key: &[u8], value: &[u8]) -> storage::Result<()> {
    let mut txn = store.new_txn(false).await?;
    txn.set(key, value).await?;
    txn.commit().await
}

fn full(error: Error) {
    assert!(
        !error.is_retriable(),
        "a failed WAL is not a transaction conflict"
    );
    let Error::Io(cause) = error else {
        panic!("typed storage failure lost")
    };
    assert_eq!(cause.kind(), std::io::ErrorKind::StorageFull);
}

#[test]
fn disk_full_remains_typed_until_reopen_and_keeps_committed_data() {
    let dir = tempfile::tempdir().unwrap();
    let env = Arc::new(FullEnv::default());
    let mut options = RegolithStoreOptions::default();
    options.engine = regolith::Options::embedded()
        .env(env.clone())
        .durability(regolith::DurabilityMode::Immediate)
        .max_background_compactions(0);
    workers::block_on(async {
        let store = RegolithStore::open_with_options(dir.path(), options.clone()).unwrap();
        write(&store, b"retained", b"committed before the fault")
            .await
            .unwrap();
        env.full.store(true, Ordering::Release);
        full(
            write(&store, b"uncertain", b"may already be in the WAL")
                .await
                .unwrap_err(),
        );
        env.full.store(false, Ordering::Release);
        full(
            write(&store, b"later", b"must not bypass the latch")
                .await
                .unwrap_err(),
        );
        drop(store);
        let reopened = RegolithStore::open_with_options(dir.path(), options).unwrap();
        let reader = reopened.new_txn(true).await.unwrap();
        assert_eq!(
            reader.get(b"retained").await.unwrap().unwrap().as_ref(),
            b"committed before the fault"
        );
        assert!(reader.get(b"later").await.unwrap().is_none());
        drop(reader);
        write(&reopened, b"recovered", b"new work").await.unwrap();
        reopened.close().await.unwrap();
    })
    .unwrap();
}
