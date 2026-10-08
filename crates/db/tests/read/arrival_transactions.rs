use super::*;
use crate::DB;
use storage::RegolithStore;

async fn cursor_of<S: Store>(db: &DB<S>, collection: u32, doc: &str) -> u64 {
    let reader = db.new_txn(true).await.unwrap();
    let cursor = number(&reader.systemstore().unwrap(), &doc_key(collection, doc))
        .await
        .unwrap();
    reader.discard().unwrap();
    cursor
}

#[tokio::test]
async fn overlapping_arrivals_both_commit_and_number_in_sequence_order() {
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    let first = db.new_txn(false).await.unwrap();
    let second = db.new_txn(false).await.unwrap();
    record(&first.systemstore().unwrap(), 1, 1, "first")
        .await
        .unwrap();
    record(&second.systemstore().unwrap(), 1, 2, "second")
        .await
        .unwrap();
    second.commit().await.unwrap();
    sequence(&db, 1).await;
    first.commit().await.unwrap();
    assert_eq!(cursor_of(&db, 1, "first").await, 0);
    sequence(&db, 1).await;
    assert_eq!(cursor_of(&db, 1, "second").await, 1);
    assert_eq!(cursor_of(&db, 1, "first").await, 2);
}

#[tokio::test]
async fn rollback_is_atomic_and_duplicate_arrivals_do_not_advance() {
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    let txn = db.new_txn(false).await.unwrap();
    record(&txn.systemstore().unwrap(), 1, 3, "rolled-back")
        .await
        .unwrap();
    txn.discard().unwrap();
    let txn = db.new_txn(false).await.unwrap();
    record(&txn.systemstore().unwrap(), 1, 4, "peer-doc")
        .await
        .unwrap();
    record(&txn.systemstore().unwrap(), 1, 4, "peer-doc")
        .await
        .unwrap();
    record(&txn.systemstore().unwrap(), 2, 5, "other-collection")
        .await
        .unwrap();
    txn.commit().await.unwrap();
    sequence(&db, 1).await;
    sequence(&db, 2).await;
    let txn = db.new_txn(false).await.unwrap();
    record(&txn.systemstore().unwrap(), 1, 4, "peer-doc")
        .await
        .unwrap();
    txn.commit().await.unwrap();
    sequence(&db, 1).await;
    assert_eq!(cursor_of(&db, 1, "peer-doc").await, 1);
    assert_eq!(cursor_of(&db, 1, "rolled-back").await, 0);
    assert_eq!(cursor_of(&db, 2, "other-collection").await, 1);
    let reader = db.new_txn(true).await.unwrap();
    assert_eq!(
        number(&reader.systemstore().unwrap(), &head_key(1))
            .await
            .unwrap(),
        1
    );
    reader.discard().unwrap();
}

#[tokio::test]
async fn arrivals_left_pending_by_a_crash_are_numbered_at_open() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = DB::new(RegolithStore::open(dir.path()).unwrap()).unwrap();
        let txn = db.new_txn(false).await.unwrap();
        record(&txn.systemstore().unwrap(), 1, 6, "before-restart")
            .await
            .unwrap();
        txn.commit().await.unwrap();
    }
    let db = DB::open(RegolithStore::open(dir.path()).unwrap())
        .await
        .unwrap();
    assert_eq!(cursor_of(&db, 1, "before-restart").await, 1);
    let txn = db.new_txn(false).await.unwrap();
    record(&txn.systemstore().unwrap(), 1, 7, "after-restart")
        .await
        .unwrap();
    txn.commit().await.unwrap();
    sequence(&db, 1).await;
    assert_eq!(cursor_of(&db, 1, "after-restart").await, 2);
}
