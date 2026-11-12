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
