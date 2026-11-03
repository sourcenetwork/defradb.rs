//! Prefix range dependencies include empty scans and work through namespaces.
use storage::corekv::{Error, IterOptions, Store};
use storage::RegolithStore;

async fn phantom_race(store: impl Store) {
    for reverse in [false, true] {
        let mut decision = store.new_txn(false).await.unwrap();
        let mut iter = decision
            .iterator(
                IterOptions::new()
                    .with_prefix(b"owners/".to_vec())
                    .with_reverse(reverse)
                    .with_validation(true),
            )
            .await
            .unwrap();
        while iter.next().await.unwrap().is_some() {}
        iter.close().await.unwrap();
        drop(iter);
        let mut concurrent = store.new_txn(false).await.unwrap();
        concurrent
            .set(if reverse { b"owners/b" } else { b"owners/a" }, b"owner")
            .await
            .unwrap();
        concurrent.commit().await.unwrap();
        decision.set(b"authorized", b"effect").await.unwrap();
        assert!(matches!(
            decision.commit().await,
            Err(Error::TxnConflictDetails(_))
        ));
    }
}

#[test]
fn exhausted_validated_ranges_detect_new_members() {
    workers::block_on(async {
        phantom_race(RegolithStore::in_memory().unwrap()).await;
        phantom_race(storage::stores::datastore::Datastore::new(
            std::sync::Arc::new(RegolithStore::in_memory().unwrap()),
        ))
        .await;
    })
    .unwrap()
}
