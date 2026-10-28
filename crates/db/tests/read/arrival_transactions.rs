use super::*;
use crate::DB;
use storage::RegolithStore;

#[test]
fn reversed_concurrent_commits_receive_cursors_in_commit_order() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let first = db.new_txn(false).await.unwrap();
        let second = db.new_txn(false).await.unwrap();
        record(&first.systemstore().unwrap(), 1, "first")
            .await
            .unwrap();
        record(&second.systemstore().unwrap(), 1, "second")
            .await
            .unwrap();
        assert_eq!(
            number(&first.systemstore().unwrap(), &head_key(1))
                .await
                .unwrap(),
            0
        );
        second.commit().await.unwrap();
        first.commit().await.unwrap();
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
    })
    .unwrap()
}

#[test]
fn rollback_is_atomic_and_duplicate_arrivals_do_not_advance() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let txn = db.new_txn(false).await.unwrap();
        record(&txn.systemstore().unwrap(), 1, "rolled-back")
            .await
            .unwrap();
        txn.discard().unwrap();
        let txn = db.new_txn(false).await.unwrap();
        record(&txn.systemstore().unwrap(), 1, "peer-doc")
            .await
            .unwrap();
        record(&txn.systemstore().unwrap(), 1, "peer-doc")
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let txn = db.new_txn(false).await.unwrap();
        record(&txn.systemstore().unwrap(), 1, "peer-doc")
            .await
            .unwrap();
        assert_eq!(
            number(&txn.systemstore().unwrap(), &doc_key(1, "rolled-back"))
                .await
                .unwrap(),
            0
        );
        record(&txn.systemstore().unwrap(), 2, "other-collection")
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let reader = db.new_txn(true).await.unwrap();
        let store = reader.systemstore().unwrap();
        assert_eq!(number(&store, &head_key(1)).await.unwrap(), 1);
        assert_eq!(number(&store, &doc_key(1, "peer-doc")).await.unwrap(), 1);
        assert_eq!(number(&store, &head_key(2)).await.unwrap(), 1);
    })
    .unwrap()
}

#[test]
fn cursor_survives_database_reopen() {
    workers::block_on(async {
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
        record(&txn.systemstore().unwrap(), 1, "before-restart")
            .await
            .unwrap();
        record(&txn.systemstore().unwrap(), 1, "after-restart")
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let reader = db.new_txn(true).await.unwrap();
        let store = reader.systemstore().unwrap();
        assert_eq!(number(&store, &head_key(1)).await.unwrap(), 2);
        assert_eq!(
            number(&store, &doc_key(1, "before-restart")).await.unwrap(),
            1
        );
        assert_eq!(
            number(&store, &doc_key(1, "after-restart")).await.unwrap(),
            2
        );
    })
    .unwrap()
}

#[test]
fn duplicate_appends_across_transactions_share_one_position() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let first = db.new_txn(false).await.unwrap();
        let second = db.new_txn(false).await.unwrap();
        record(&first.systemstore().unwrap(), 1, "same")
            .await
            .unwrap();
        record(&second.systemstore().unwrap(), 1, "same")
            .await
            .unwrap();
        first.commit().await.unwrap();
        second.commit().await.unwrap();
        let reader = db.new_txn(true).await.unwrap();
        assert_eq!(
            number(&reader.systemstore().unwrap(), &head_key(1))
                .await
                .unwrap(),
            1
        );
    })
    .unwrap()
}
