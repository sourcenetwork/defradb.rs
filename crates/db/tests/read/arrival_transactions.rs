use super::*;
use crate::DB;
use storage::RegolithStore;

#[tokio::test]
async fn reversed_concurrent_commits_retry_without_late_lower_cursor() {
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    let first = db.new_txn(false).await.unwrap();
    let second = db.new_txn(false).await.unwrap();
    assert_eq!(
        record(&first.systemstore().unwrap(), 1, "first")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        record(&second.systemstore().unwrap(), 1, "second")
            .await
            .unwrap(),
        1
    );
    second.commit().await.unwrap();
    assert!(first.commit().await.is_err());
    let retry = db.new_txn(false).await.unwrap();
    assert_eq!(
        record(&retry.systemstore().unwrap(), 1, "first")
            .await
            .unwrap(),
        2
    );
    retry.commit().await.unwrap();
    let reader = db.new_txn(true).await.unwrap();
    assert_eq!(
        number(&reader.systemstore().unwrap(), &doc_key(1, "second"))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        number(&reader.systemstore().unwrap(), &doc_key(1, "first"))
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn rollback_is_atomic_and_duplicate_arrivals_do_not_advance() {
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    let txn = db.new_txn(false).await.unwrap();
    record(&txn.systemstore().unwrap(), 1, "rolled-back")
        .await
        .unwrap();
    txn.discard().unwrap();
    let txn = db.new_txn(false).await.unwrap();
    assert_eq!(
        record(&txn.systemstore().unwrap(), 1, "peer-doc")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        record(&txn.systemstore().unwrap(), 1, "peer-doc")
            .await
            .unwrap(),
        1
    );
    txn.commit().await.unwrap();
    let txn = db.new_txn(false).await.unwrap();
    assert_eq!(
        record(&txn.systemstore().unwrap(), 1, "peer-doc")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        number(&txn.systemstore().unwrap(), &doc_key(1, "rolled-back"))
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        record(&txn.systemstore().unwrap(), 2, "other-collection")
            .await
            .unwrap(),
        1
    );
    txn.commit().await.unwrap();
}

#[tokio::test]
async fn cursor_survives_database_reopen() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = DB::new(RegolithStore::open(dir.path()).unwrap()).unwrap();
        let txn = db.new_txn(false).await.unwrap();
        record(&txn.systemstore().unwrap(), 1, "before-restart")
            .await
            .unwrap();
        txn.commit().await.unwrap();
    }
    let db = DB::new(RegolithStore::open(dir.path()).unwrap()).unwrap();
    let txn = db.new_txn(false).await.unwrap();
    assert_eq!(
        record(&txn.systemstore().unwrap(), 1, "before-restart")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        record(&txn.systemstore().unwrap(), 1, "after-restart")
            .await
            .unwrap(),
        2
    );
    txn.commit().await.unwrap();
}
