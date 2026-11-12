use super::*;
use crate::DB;
use storage::RegolithStore;

#[test]
fn identities_are_unambiguous_and_payloads_round_trip() {
    assert_ne!(once_key(1, "a", "bc"), once_key(1, "ab", "c"));
    assert_ne!(once_key(1, "a", "x"), once_key(1, "a", "y"));
    assert_ne!(once_key(1, "a", "x"), once_key(2, "a", "x"));
    let update = Update {
        doc_id: "a/☃".into(),
        cid: "cid".into(),
    };
    assert_eq!(
        serde_json::from_slice::<Update>(&serde_json::to_vec(&update).unwrap()).unwrap(),
        update
    );
}

#[test]
fn commit_order_rollback_and_replayed_updates() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let first = db.new_txn(false).await.unwrap();
        let second = db.new_txn(false).await.unwrap();
        let discarded = db.new_txn(false).await.unwrap();
        record(&first.systemstore().unwrap(), 1, "doc", "cid-1")
            .await
            .unwrap();
        record(&second.systemstore().unwrap(), 1, "doc", "cid-2")
            .await
            .unwrap();
        record(&discarded.systemstore().unwrap(), 1, "doc", "discarded")
            .await
            .unwrap();
        discarded.discard().unwrap();
        second.commit().await.unwrap();
        first.commit().await.unwrap();
        let replay = db.new_txn(false).await.unwrap();
        record(&replay.systemstore().unwrap(), 1, "doc", "cid-2")
            .await
            .unwrap();
        replay.commit().await.unwrap();
        let reader = db.new_txn(true).await.unwrap();
        let store = reader.systemstore().unwrap();
        let head = store
            .get(b"/arrival/updates/1/head")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(u64::from_be_bytes(head.as_ref().try_into().unwrap()), 2);
        for (cursor, cid) in [(1, "cid-2"), (2, "cid-1")] {
            let key = format!("/arrival/updates/1/row/{cursor:020}");
            let row = store.get(key.as_bytes()).await.unwrap().unwrap();
            assert_eq!(serde_json::from_slice::<Update>(&row).unwrap().cid, cid);
        }
    })
    .unwrap();
}

#[test]
fn updates_and_replay_checkpoint_survive_reopen() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = DB::new(RegolithStore::open(dir.path()).unwrap()).unwrap();
            let txn = db.new_txn(false).await.unwrap();
            record(&txn.systemstore().unwrap(), 1, "doc", "cid-1")
                .await
                .unwrap();
            txn.commit().await.unwrap();
        }
        let db = DB::new(RegolithStore::open(dir.path()).unwrap()).unwrap();
        let txn = db.new_txn(false).await.unwrap();
        record(&txn.systemstore().unwrap(), 1, "doc", "cid-1")
            .await
            .unwrap();
        record(&txn.systemstore().unwrap(), 1, "doc", "cid-2")
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let store = txn.systemstore().unwrap();
        let head = store
            .get(b"/arrival/updates/1/head")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(u64::from_be_bytes(head.as_ref().try_into().unwrap()), 2);
        let row = store
            .get(b"/arrival/updates/1/row/00000000000000000002")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::from_slice::<Update>(&row).unwrap().cid, "cid-2");
    })
    .unwrap();
}

#[test]
fn a_scan_returns_the_rows_of_its_collection_and_range_only() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        // Collections 1 and 12: one's row prefix is not the other's.
        for (collection, cid) in [(1, "a"), (12, "x"), (1, "b"), (12, "y"), (1, "c")] {
            let txn = db.new_txn(false).await.unwrap();
            record(&txn.systemstore().unwrap(), collection, "doc", cid)
                .await
                .unwrap();
            txn.commit().await.unwrap();
        }
        let reader = db.new_txn(true).await.unwrap();
        let store = reader.systemstore().unwrap();
        let cids = |rows: Vec<(u64, Update)>| -> Vec<(u64, String)> {
            rows.into_iter()
                .map(|(cursor, row)| (cursor, row.cid))
                .collect()
        };
        assert_eq!(
            cids(rows(&store, 1, 0, 3).await.unwrap()),
            [(1, "a".into()), (2, "b".into()), (3, "c".into())]
        );
        assert_eq!(
            cids(rows(&store, 1, 1, 2).await.unwrap()),
            [(2, "b".into())]
        );
        assert_eq!(
            cids(rows(&store, 12, 0, 2).await.unwrap()),
            [(1, "x".into()), (2, "y".into())]
        );
        assert!(rows(&store, 1, 3, 3).await.unwrap().is_empty());
        // A range past the head finds no row where one is due, and never
        // another collection's.
        let missing = rows(&store, 1, 0, 4).await.unwrap_err();
        assert!(
            missing.to_string().contains("missing update row"),
            "{missing}"
        );
    })
    .unwrap();
}
