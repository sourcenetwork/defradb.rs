use std::sync::Arc;

use super::*;
use crate::event::updates::{record, rows};
use crate::DB;
use storage::RegolithStore;

async fn journal(rows: u64) -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let txn = db.new_txn(false).await.unwrap();
    for n in 1..=rows {
        record(&txn.systemstore().unwrap(), 1, "doc", &format!("cid-{n}"))
            .await
            .unwrap();
    }
    txn.commit().await.unwrap();
    db
}

fn says_read_after(error: &Error, oldest: u64) {
    let text = error.to_string();
    assert!(
        text.contains(&format!(
            "update cursor is no longer available on this node: resync the collection and read after cursor {oldest}"
        )),
        "{text}"
    );
}

#[test]
fn a_full_drop_leaves_a_cursor_before_it_unavailable() {
    workers::block_on(async {
        let db = journal(3).await;
        drop_rows(&db, 1).await.unwrap();
        let reader = db.new_txn(true).await.unwrap();
        let store = reader.systemstore().unwrap();
        for after in 0..3 {
            says_read_after(&rows(&store, 1, after, 3).await.unwrap_err(), 3);
        }
        assert!(rows(&store, 1, 3, 3).await.unwrap().is_empty());
        drop(store);
        let _ = reader.discard();

        // The head kept counting: the next update is cursor 4, not 1.
        let txn = db.new_txn(false).await.unwrap();
        record(&txn.systemstore().unwrap(), 1, "doc", "cid-4")
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let reader = db.new_txn(true).await.unwrap();
        let store = reader.systemstore().unwrap();
        let page = rows(&store, 1, 3, 4).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].0, 4);
        assert_eq!(page[0].1.cid, "cid-4");
        says_read_after(&rows(&store, 1, 1, 4).await.unwrap_err(), 3);
    })
    .unwrap();
}

#[test]
fn slices_of_a_long_journal_drop_every_row() {
    workers::block_on(async {
        let db = journal(2 * SLICE + 7).await;
        drop_rows(&db, 1).await.unwrap();
        // A second drop finds nothing left to drop.
        drop_rows(&db, 1).await.unwrap();
        let reader = db.new_txn(true).await.unwrap();
        let store = reader.systemstore().unwrap();
        assert_eq!(oldest_row(&store, 1).await.unwrap(), None);
        assert_eq!(stored_head(&store, 1).await.unwrap(), 2 * SLICE + 7);
        says_read_after(
            &rows(&store, 1, SLICE, SLICE + 1).await.unwrap_err(),
            2 * SLICE + 7,
        );
    })
    .unwrap();
}

#[test]
fn a_drop_keeps_the_rows_after_the_head_it_read() {
    workers::block_on(async {
        let db = journal(2).await;
        let txn = db.new_txn(false).await.unwrap();
        record(&txn.systemstore().unwrap(), 1, "doc", "cid-3")
            .await
            .unwrap();
        // Committed after the drop read its head, so the drop keeps it.
        drop_rows(&db, 1).await.unwrap();
        txn.commit().await.unwrap();
        let reader = db.new_txn(true).await.unwrap();
        let store = reader.systemstore().unwrap();
        assert_eq!(oldest_row(&store, 1).await.unwrap(), Some(3));
        says_read_after(&rows(&store, 1, 0, 3).await.unwrap_err(), 2);
        let page = rows(&store, 1, 2, 3).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].1.cid, "cid-3");
    })
    .unwrap();
}

#[test]
fn a_hole_below_kept_rows_is_a_missing_row() {
    workers::block_on(async {
        let db = journal(3).await;
        let txn = db.new_txn(false).await.unwrap();
        txn.systemstore()
            .unwrap()
            .truncate_log(&log(1), 2..=2)
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let reader = db.new_txn(true).await.unwrap();
        let store = reader.systemstore().unwrap();
        for after in [0, 1] {
            let error = rows(&store, 1, after, 3).await.unwrap_err();
            assert!(error.to_string().contains("missing update row"), "{error}");
        }
    })
    .unwrap();
}
