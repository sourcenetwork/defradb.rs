//! The orphan block sweep (`go.shinzo.orphan_gc`) and the refresh that keeps
//! a block still arriving out of it (`go.shinzo.merge.refetch`).

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use cid::Cid;
use db::block::orphan_gc::SweepReport;
use db::DB;
use defra_core::block::generate_cid_from_bytes;
use storage::corekv::{Key, Writer};
use storage::keys::blockstore::ToMergeIndexKey;
use storage::stores::blockstore::{marker_time, marker_value, BlockstoreTxn};
use storage::RegolithStore;

/// The cutoff the tests sweep at, in unix seconds.
const CUTOFF: u64 = 1_800_000_000;
const HOUR: u64 = 3600;

/// A node's database and its P2P blockstore over one store.
fn node() -> (Arc<DB<RegolithStore>>, DefraBlockstore<RegolithStore>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    (db, DefraBlockstore::new(store, true))
}

fn cid(name: &str) -> Cid {
    generate_cid_from_bytes(name.as_bytes()).unwrap()
}

/// Store block `name` as a fetch would, with its mark set to `mark`.
async fn fetched(blockstore: &DefraBlockstore<RegolithStore>, name: &str, mark: &[u8]) -> Cid {
    let block = cid(name);
    blockstore.put(&block, name.as_bytes()).await.unwrap();
    let mut txn = blockstore.new_store_txn(false).await.unwrap();
    txn.as_any_mut()
        .downcast_mut::<BlockstoreTxn>()
        .unwrap()
        .set(&ToMergeIndexKey::new(block).bytes(), mark)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    block
}

/// The fetch time `block`'s mark holds, or `None` when it has no mark.
async fn mark_of(blockstore: &DefraBlockstore<RegolithStore>, block: &Cid) -> Option<Option<u64>> {
    let txn = blockstore.new_store_txn(true).await.unwrap();
    let mark = storage::corekv::Reader::get(&*txn, &ToMergeIndexKey::new(*block).bytes())
        .await
        .unwrap();
    mark.map(|value| marker_time(&value))
}

async fn sweep(db: &Arc<DB<RegolithStore>>, start: Option<Vec<u8>>, limit: usize) -> SweepReport {
    db.reclaim_orphan_blocks(CUTOFF, start, limit, &AtomicBool::new(false))
        .await
        .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.orphan_gc.reclaim")]
fn only_unowned_blocks_fetched_before_the_cutoff_are_reclaimed() {
    workers::block_on(async {
        let (db, blockstore) = node();
        let old = fetched(&blockstore, "old", &marker_value(CUTOFF - HOUR)).await;
        let recent = fetched(&blockstore, "recent", &marker_value(CUTOFF + HOUR)).await;
        let untimed = fetched(&blockstore, "untimed", &[0xff]).await;
        let merged = fetched(&blockstore, "merged", &marker_value(CUTOFF - HOUR)).await;
        blockstore.mark_as_merged(&merged).await.unwrap();

        let report = sweep(&db, None, 100).await;
        assert_eq!(
            report,
            SweepReport {
                scanned: 3,
                reclaimed: 1,
                ..SweepReport::default()
            }
        );
        assert!(!blockstore.has(&old).await.unwrap());
        assert_eq!(
            mark_of(&blockstore, &old).await,
            None,
            "its mark goes with it"
        );
        for kept in [recent, untimed, merged] {
            assert!(blockstore.has(&kept).await.unwrap());
        }

        // A block past the cutoff that a document owns is kept and marked merged.
        let owned = fetched(&blockstore, "owned", &marker_value(CUTOFF - HOUR)).await;
        let txn = db.new_txn(false).await.unwrap();
        db::docid::map::set_block_doc_id_mapping(
            &txn.systemstore().unwrap(),
            &owned.to_string(),
            "bae-owner",
        )
        .await
        .unwrap();
        txn.commit().await.unwrap();
        let report = sweep(&db, None, 100).await;
        assert_eq!((report.reclaimed, report.repaired), (0, 1));
        assert!(blockstore.has(&owned).await.unwrap());
        assert!(blockstore.is_merged(&owned).await.unwrap());
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.orphan_gc.race")]
fn a_merge_committing_after_the_nomination_keeps_its_block() {
    workers::block_on(async {
        let (db, blockstore) = node();
        let block = fetched(&blockstore, "nominated", &marker_value(CUTOFF - HOUR)).await;
        // The sweep has nominated the block; its merge commits before the decision.
        blockstore.mark_as_merged(&block).await.unwrap();
        let report = db.decide_orphans(CUTOFF, vec![block]).await.unwrap();
        assert_eq!(report.reclaimed, 0);
        assert!(blockstore.has(&block).await.unwrap());
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.orphan_gc.resume")]
fn the_sweep_resumes_across_calls_and_stops_when_cancelled() {
    workers::block_on(async {
        let (db, blockstore) = node();
        let mut blocks = Vec::new();
        for i in 0..5 {
            blocks.push(
                fetched(
                    &blockstore,
                    &format!("expired-{i}"),
                    &marker_value(CUTOFF - HOUR),
                )
                .await,
            );
        }
        let cancelled = db
            .reclaim_orphan_blocks(CUTOFF, None, 2, &AtomicBool::new(true))
            .await
            .unwrap_err();
        assert!(
            cancelled.to_string().contains("context canceled"),
            "{cancelled}"
        );
        for block in &blocks {
            assert!(
                blockstore.has(block).await.unwrap(),
                "a cancelled sweep deletes nothing"
            );
        }

        let (mut calls, mut scanned, mut reclaimed, mut start) = (0, 0, 0, None);
        loop {
            let report = sweep(&db, start, 2).await;
            calls += 1;
            scanned += report.scanned;
            reclaimed += report.reclaimed;
            match report.next_key {
                Some(next) => start = Some(next),
                None => break,
            }
        }
        assert!(calls > 1);
        assert_eq!((scanned, reclaimed), (5, 5));
        for block in &blocks {
            assert!(!blockstore.has(block).await.unwrap());
        }
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.merge.refetch")]
fn storing_a_waiting_block_again_renews_its_mark_and_a_merged_one_stays_merged() {
    workers::block_on(async {
        let (_db, blockstore) = node();
        let waiting = fetched(&blockstore, "waiting", &marker_value(1)).await;
        blockstore.put(&waiting, b"waiting").await.unwrap();
        let renewed = mark_of(&blockstore, &waiting).await.flatten().unwrap();
        assert!(
            renewed > CUTOFF - HOUR * 24 * 365 * 10,
            "renewed to now, got {renewed}"
        );

        let merged = fetched(&blockstore, "merged", &marker_value(1)).await;
        blockstore.mark_as_merged(&merged).await.unwrap();
        blockstore.put(&merged, b"merged").await.unwrap();
        assert!(blockstore.is_merged(&merged).await.unwrap());

        // A batch put holding one waiting and one merged block renews the
        // first and leaves the second merged.
        let again = fetched(&blockstore, "again", &marker_value(1)).await;
        blockstore
            .put_many(&[(&again, &b"again"[..]), (&merged, &b"merged"[..])])
            .await
            .unwrap();
        assert!(mark_of(&blockstore, &again).await.flatten().unwrap() > 1);
        assert!(blockstore.is_merged(&merged).await.unwrap());
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.merge.refetch")]
fn a_merge_racing_a_refetch_always_leaves_the_block_merged() {
    workers::block_on(async {
        let (_db, blockstore) = node();
        for i in 0..300 {
            let name = format!("raced-{i}");
            let block = fetched(&blockstore, &name, &marker_value(1)).await;
            let (merged, put) = futures::join!(
                blockstore.mark_as_merged(&block),
                blockstore.put(&block, name.as_bytes())
            );
            merged.unwrap();
            put.unwrap();
            assert!(blockstore.is_merged(&block).await.unwrap(), "race {i}");
        }
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.store.orphan_sweep_config")]
fn the_sweep_is_configured_from_the_environment() {
    use db::block::orphan_gc::OrphanSweepConfig;
    use std::time::Duration;
    let with = |pairs: &[(&str, &str)]| {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        OrphanSweepConfig::from_vars(move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        })
    };
    let defaults = OrphanSweepConfig {
        enabled: true,
        interval: Duration::from_secs(300),
        ttl: Duration::from_secs(1800),
    };
    assert_eq!(with(&[]), defaults);
    for off in ["1", "t", "T", "TRUE", "true", "True"] {
        assert!(!with(&[("DEFRA_ORPHAN_GC_DISABLED", off)]).enabled, "{off}");
    }
    for on in ["0", "f", "F", "FALSE", "false", "False", "yes", "off", ""] {
        assert!(with(&[("DEFRA_ORPHAN_GC_DISABLED", on)]).enabled, "{on:?}");
    }
    let set = with(&[
        ("DEFRA_ORPHAN_GC_INTERVAL", "3m"),
        ("DEFRA_ORPHAN_BLOCK_TTL", "90s"),
    ]);
    assert_eq!(
        (set.interval, set.ttl),
        (Duration::from_secs(180), Duration::from_secs(90))
    );
    for bad in ["", "not-a-duration", "0s", "-1m"] {
        let fallback = with(&[
            ("DEFRA_ORPHAN_GC_INTERVAL", bad),
            ("DEFRA_ORPHAN_BLOCK_TTL", bad),
        ]);
        assert_eq!(
            (fallback.interval, fallback.ttl),
            (defaults.interval, defaults.ttl),
            "{bad:?}"
        );
    }
}

#[test]
#[mantra_macros::req_test("go.shinzo.store.orphan_sweep_config")]
fn with_the_sweep_on_an_abandoned_fetch_goes_once_its_mark_ages() {
    use db::block::orphan_gc::{run_orphan_sweep, OrphanSweepConfig};
    use std::time::Duration;
    workers::block_on(async {
        let (db, blockstore) = node();
        let abandoned = cid("abandoned");
        blockstore.put(&abandoned, b"abandoned").await.unwrap();
        let merged = cid("merged-in-time");
        blockstore.put(&merged, b"merged-in-time").await.unwrap();
        blockstore.mark_as_merged(&merged).await.unwrap();
        let config = OrphanSweepConfig {
            enabled: true,
            interval: Duration::from_millis(500),
            ttl: Duration::from_secs(1),
        };
        // Long enough for the mark to age a whole second past the TTL.
        let stop = workers::delay(Duration::from_millis(3500)).unwrap();
        run_orphan_sweep(db.clone(), config, async move {
            let _ = stop.await;
        })
        .await;
        assert!(
            !blockstore.has(&abandoned).await.unwrap(),
            "the abandoned block is swept"
        );
        assert!(
            blockstore.has(&merged).await.unwrap(),
            "the merged block stays"
        );
    })
    .unwrap()
}
