//! Projected read semantics survive the namespace and boxed-transaction layers.
use std::sync::Arc;
use storage::corekv::{Error, Reader, Store};
use storage::RegolithStore;

async fn check_projection(store: impl Store) {
    for (parts, validates) in [([].as_slice(), false), ([1].as_slice(), true)] {
        let mut seed = store.new_txn(false).await.unwrap();
        seed.set(b"value", b"before").await.unwrap();
        seed.commit().await.unwrap();
        let mut reader = store.new_txn(false).await.unwrap();
        assert_eq!(
            Reader::get_parts(&reader, b"value", parts)
                .await
                .unwrap()
                .as_deref(),
            Some(b"before".as_slice())
        );
        let mut writer = store.new_txn(false).await.unwrap();
        writer.set(b"value", b"after").await.unwrap();
        writer.commit().await.unwrap();
        reader.set(b"effect", b"written").await.unwrap();
        let result = reader.commit().await;
        if validates {
            assert!(matches!(result, Err(Error::TxnConflictDetails(_))));
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn projected_reads_through_store_wrappers() {
    workers::block_on(async {
        check_projection(RegolithStore::in_memory().unwrap()).await;
        check_projection(storage::stores::datastore::Datastore::new(Arc::new(
            RegolithStore::in_memory().unwrap(),
        )))
        .await;
        check_projection(storage::stores::blockstore::Blockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            false,
        ))
        .await;
    })
    .unwrap()
}
