//! Snapshot-coherent materialization of the two collection-head prefixes.
//!
//! `HeadSet.Cache` proves publication and cold-fill rules. This is a cache of
//! raw rows, including orphan markers, not a second maintained head set.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use bytes::Bytes;
use regolith::Snapshot;

use crate::corekv::{Error, KvPair, Result};
use crate::keys::headstore::{HeadstoreColKey, HeadstoreColSuperseded};
use crate::stores::headstore::CollectionHeadEntries;

const CACHE_BYTES: usize = 256 * 1024;
type Rows = BTreeMap<Vec<u8>, Bytes>;
type Collections = BTreeMap<u32, Arc<Rows>>;

fn row_bytes(key: &[u8], value: &[u8]) -> usize {
    key.len() + value.len() + 96
}

fn rows_bytes(rows: &Rows) -> usize {
    rows.iter().map(|(key, value)| row_bytes(key, value)).sum()
}

fn collection(key: &[u8]) -> Option<u32> {
    let rest = key
        .strip_prefix(b"h/c/")
        .or_else(|| key.strip_prefix(b"h/cs/"))?;
    let end = rest.iter().position(|byte| *byte == b'/')?;
    let text = std::str::from_utf8(&rest[..end]).ok()?;
    let id: u32 = text.parse().ok()?;
    (text == id.to_string()).then_some(id)
}

pub(super) fn matching_collection(head_prefix: &[u8], marker_prefix: &[u8]) -> Option<u32> {
    let id = collection(head_prefix)?;
    (head_prefix == [b"h".as_slice(), &HeadstoreColKey::collection_prefix(id)].concat()
        && marker_prefix
            == [
                b"h".as_slice(),
                &HeadstoreColSuperseded::collection_prefix(id),
            ]
            .concat())
    .then_some(id)
}

fn admit(all: &mut Arc<Collections>, id: u32, rows: Arc<Rows>) {
    let size = rows_bytes(&rows) + 128;
    let all = Arc::make_mut(all);
    all.remove(&id);
    if size > CACHE_BYTES {
        return;
    }
    while all
        .values()
        .map(|rows| rows_bytes(rows) + 128)
        .sum::<usize>()
        + size
        > CACHE_BYTES
    {
        all.pop_first();
    }
    all.insert(id, rows);
}

#[derive(Default)]
pub(crate) struct HeadCache {
    pub(super) epoch: u64,
    pub(super) disabled: bool,
    reset_epoch: u64,
    rows: Arc<Collections>,
    #[cfg(test)]
    cold_scans: usize,
}

impl HeadCache {
    pub(super) fn capture(&self, snapshot: Snapshot) -> HeadSnapshot {
        HeadSnapshot {
            snapshot,
            epoch: self.epoch,
            reset_epoch: self.reset_epoch,
            rows: Mutex::new(Arc::clone(&self.rows)),
        }
    }

    pub(super) fn invalidate(&mut self) {
        self.rows = Arc::default();
        self.advance();
    }

    fn advance(&mut self) {
        match self.epoch.checked_add(1) {
            Some(epoch) => self.epoch = epoch,
            None => self.disabled = true,
        }
    }

    pub(super) fn reset(&mut self) {
        self.invalidate();
        self.reset_epoch = self.epoch;
    }

    pub(super) fn disable(&mut self) {
        self.disabled = true;
        self.invalidate();
    }

    pub(super) fn publish(&mut self, changes: &HeadChanges) {
        let Some(writes) = &changes.writes else {
            self.invalidate();
            return;
        };
        if writes.is_empty() {
            return;
        }
        self.advance();
        if self.disabled {
            return;
        }
        let mut updates: BTreeMap<u32, Rows> = BTreeMap::new();
        for (key, value) in writes {
            let Some(id) = collection(key) else { continue };
            let Some(current) = self.rows.get(&id) else {
                continue;
            };
            let rows = updates.entry(id).or_insert_with(|| (**current).clone());
            apply(rows, key, value);
        }
        for (id, rows) in updates {
            admit(&mut self.rows, id, Arc::new(rows));
        }
    }
}

pub(crate) type SharedHeadCache = Arc<Mutex<HeadCache>>;

pub(super) fn lock(cache: &SharedHeadCache) -> Result<MutexGuard<'_, HeadCache>> {
    cache
        .lock()
        .map_err(|_| Error::Backend("collection head cache publication poisoned".into()))
}

pub(super) struct HeadSnapshot {
    snapshot: Snapshot,
    epoch: u64,
    reset_epoch: u64,
    rows: Mutex<Arc<Collections>>,
}

impl HeadSnapshot {
    pub(super) fn read(
        &self,
        cache: &SharedHeadCache,
        changes: &HeadChanges,
        id: u32,
    ) -> Result<Option<CollectionHeadEntries>> {
        super::blocking(|| {
            {
                let current = lock(cache)?;
                // Native drop_all resets sequence numbers and invalidates old
                // snapshots. Those readers must fall back to the native owner too.
                if current.disabled || current.reset_epoch != self.reset_epoch {
                    return Ok(None);
                }
            }
            let Some(writes) = &changes.writes else {
                return Ok(None);
            };
            let cached = self
                .rows
                .lock()
                .map_err(|_| Error::Backend("head snapshot poisoned".into()))?
                .get(&id)
                .cloned();
            let base = match cached {
                Some(rows) => rows,
                None => {
                    let mut rows = Rows::new();
                    let mut size = 0;
                    #[cfg(test)]
                    {
                        lock(cache)?.cold_scans += 1;
                    }
                    for prefix in [
                        HeadstoreColKey::collection_prefix(id),
                        HeadstoreColSuperseded::collection_prefix(id),
                    ] {
                        let prefix = [b"h".as_slice(), &prefix].concat();
                        let mut cursor = self.snapshot.owned_iter();
                        cursor.seek_prefix(&prefix);
                        while cursor.valid() {
                            let key = cursor.key().expect("valid cursor key").to_vec();
                            let value =
                                Bytes::copy_from_slice(cursor.value().expect("valid cursor value"));
                            size += row_bytes(&key, &value);
                            if size > CACHE_BYTES {
                                return Ok(None);
                            }
                            rows.insert(key, value);
                            cursor.next();
                        }
                        cursor.status().map_err(|e| Error::Backend(e.to_string()))?;
                    }
                    let rows = Arc::new(rows);
                    {
                        let mut current = lock(cache)?;
                        if !current.disabled && self.epoch == current.epoch {
                            admit(&mut current.rows, id, Arc::clone(&rows));
                        }
                    }
                    let mut local = self
                        .rows
                        .lock()
                        .map_err(|_| Error::Backend("head snapshot poisoned".into()))?;
                    admit(&mut local, id, Arc::clone(&rows));
                    rows
                }
            };
            let mut rows = (*base).clone();
            for (key, value) in writes {
                if collection(key) == Some(id) {
                    apply(&mut rows, key, value);
                }
            }
            let mut entries = CollectionHeadEntries::default();
            for (key, value) in rows {
                let head = key.starts_with(b"h/c/");
                let pair = KvPair { key, value };
                if head {
                    entries.heads.push(pair);
                } else {
                    entries.markers.push(pair);
                }
            }
            Ok(Some(entries))
        })
    }
}

fn apply(rows: &mut Rows, key: &[u8], value: &Option<Bytes>) {
    match value {
        Some(value) => {
            rows.insert(key.to_vec(), value.clone());
        }
        None => {
            rows.remove(key);
        }
    }
}

/// The native transaction remains the write buffer and conflict owner. This
/// bounded journal only updates cached rows after a successful native commit;
/// exceeding its budget evicts the cache rather than retaining a second buffer.
pub(super) struct HeadChanges {
    writes: Option<BTreeMap<Vec<u8>, Option<Bytes>>>,
    bytes: usize,
}

impl Default for HeadChanges {
    fn default() -> Self {
        Self {
            writes: Some(BTreeMap::new()),
            bytes: 0,
        }
    }
}

impl HeadChanges {
    pub(super) fn record(&mut self, key: &[u8], value: Option<&[u8]>) {
        if collection(key).is_none() {
            return;
        }
        let Some(writes) = &mut self.writes else {
            return;
        };
        if let Some(old) = writes.get(key) {
            self.bytes -= row_bytes(key, old.as_deref().unwrap_or_default());
        }
        self.bytes += row_bytes(key, value.unwrap_or_default());
        if self.bytes > CACHE_BYTES {
            self.writes = None;
            return;
        }
        writes.insert(key.to_vec(), value.map(Bytes::copy_from_slice));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::regolith::{RegolithStore, RegolithStoreOptions, RegolithTxn};
    use crate::corekv::{Dropable, Store, Txn};

    fn cache(txn: &dyn Txn) -> SharedHeadCache {
        txn.as_any()
            .downcast_ref::<RegolithTxn>()
            .unwrap()
            .head_cache
            .clone()
    }

    async fn head_entries(txn: &dyn Txn, id: u32) -> Result<Option<CollectionHeadEntries>> {
        txn.collection_head_entries(
            format!("h/c/{id}/").as_bytes(),
            format!("h/cs/{id}/").as_bytes(),
        )
        .await
    }

    async fn rows(txn: &dyn Txn) -> CollectionHeadEntries {
        head_entries(txn, 1).await.unwrap().expect("cache enabled")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cache_publication_wait_does_not_starve_executor() {
        let store = RegolithStore::in_memory().unwrap();
        let first = store.new_txn(true).await.unwrap();
        let shared = cache(first.as_ref());
        first.discard();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _guard = lock(&shared).unwrap();
            locked_tx.send(()).unwrap();
            let _ = release_rx.recv_timeout(std::time::Duration::from_secs(2));
        });
        locked_rx.recv().unwrap();
        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel(2);
        let mut readers = Vec::new();
        for _ in 0..2 {
            let store = store.clone();
            let started = started_tx.clone();
            readers.push(tokio::spawn(async move {
                started.send(()).await.unwrap();
                store.new_txn(true).await.unwrap().discard();
            }));
        }
        started_rx.recv().await.unwrap();
        started_rx.recv().await.unwrap();
        let start = std::time::Instant::now();
        tokio::spawn(async {}).await.unwrap();
        let elapsed = start.elapsed();
        let _ = release_tx.send(());
        holder.join().unwrap();
        for reader in readers {
            reader.await.unwrap();
        }
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "unrelated task blocked for {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn readonly_commit_does_not_wait_for_publication() {
        let store = RegolithStore::in_memory().unwrap();
        let txn = store.new_txn(true).await.unwrap();
        let shared = cache(txn.as_ref());
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _guard = lock(&shared).unwrap();
            locked_tx.send(()).unwrap();
            let _ = release_rx.recv_timeout(std::time::Duration::from_secs(2));
        });
        locked_rx.recv().unwrap();
        let start = std::time::Instant::now();
        txn.commit().await.unwrap();
        let elapsed = start.elapsed();
        let _ = release_tx.send(());
        holder.join().unwrap();
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "read-only completion blocked for {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn warm_reads_do_not_rescan_tombstone_history() {
        let store = RegolithStore::in_memory().unwrap();
        let first = store.new_txn(false).await.unwrap();
        let shared = cache(first.as_ref());
        assert!(rows(first.as_ref()).await.heads.is_empty());
        first.discard();
        for n in 0..2048 {
            let mut txn = store.new_txn(false).await.unwrap();
            let before = rows(txn.as_ref()).await;
            assert_eq!(before.heads.len(), usize::from(n > 0));
            if n > 0 {
                txn.delete(format!("h/c/1/{:08}", n - 1).as_bytes())
                    .await
                    .unwrap();
            }
            txn.set(format!("h/c/1/{n:08}").as_bytes(), b"priority")
                .await
                .unwrap();
            txn.commit().await.unwrap();
        }
        let txn = store.new_txn(true).await.unwrap();
        assert_eq!(rows(txn.as_ref()).await.heads[0].key, b"h/c/1/00002047");
        assert_eq!(lock(&shared).unwrap().cold_scans, 1);
    }

    #[tokio::test]
    async fn stale_fill_and_failed_commit_cannot_publish() {
        let store = RegolithStore::in_memory().unwrap();
        let stale = store.new_txn(true).await.unwrap();
        let mut first = store.new_txn(false).await.unwrap();
        let mut losing = store.new_txn(false).await.unwrap();
        first.set(b"h/c/1/a", b"1").await.unwrap();
        first.set(b"d/conflict", b"first").await.unwrap();
        losing.set(b"h/c/1/b", b"2").await.unwrap();
        losing.set(b"d/conflict", b"loser").await.unwrap();
        assert_eq!(rows(losing.as_ref()).await.heads.len(), 1);
        first.commit().await.unwrap();
        assert!(rows(stale.as_ref()).await.heads.is_empty());
        assert!(matches!(losing.commit().await, Err(Error::TxnConflict)));
        let now = store.new_txn(true).await.unwrap();
        assert_eq!(rows(now.as_ref()).await.heads[0].key, b"h/c/1/a");
        assert_eq!(rows(now.as_ref()).await.heads.len(), 1);
    }

    #[tokio::test]
    async fn eviction_keeps_old_snapshots_and_raw_root_writes_update_new_ones() {
        let store = RegolithStore::in_memory().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        seed.set(b"h/c/1/a", b"1").await.unwrap();
        seed.commit().await.unwrap();
        let old = store.new_txn(true).await.unwrap();
        assert_eq!(rows(old.as_ref()).await.heads.len(), 1);
        let shared = cache(old.as_ref());
        lock(&shared).unwrap().invalidate();
        let mut writer = store.new_txn(false).await.unwrap();
        writer.delete(b"h/c/1/a").await.unwrap();
        writer.set(b"h/c/1/b", b"2").await.unwrap();
        writer.commit().await.unwrap();
        assert_eq!(rows(old.as_ref()).await.heads[0].key, b"h/c/1/a");
        let now = store.new_txn(true).await.unwrap();
        assert_eq!(rows(now.as_ref()).await.heads[0].key, b"h/c/1/b");
    }

    #[tokio::test]
    async fn budget_falls_back_and_commit_evicts_oversized_journal() {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        let shared = cache(txn.as_ref());
        rows(txn.as_ref()).await;
        txn.set(b"h/c/1/large", &vec![0; CACHE_BYTES + 1])
            .await
            .unwrap();
        assert!(head_entries(txn.as_ref(), 1).await.unwrap().is_none());
        txn.commit().await.unwrap();
        assert!(lock(&shared).unwrap().rows.is_empty());
        let txn = store.new_txn(true).await.unwrap();
        assert!(head_entries(txn.as_ref(), 1).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn empty_collections_are_bounded_too() {
        let store = RegolithStore::in_memory().unwrap();
        let txn = store.new_txn(true).await.unwrap();
        for id in 0..4096 {
            assert!(head_entries(txn.as_ref(), id)
                .await
                .unwrap()
                .unwrap()
                .heads
                .is_empty());
        }
        let shared = cache(txn.as_ref());
        assert_eq!(lock(&shared).unwrap().rows.len(), CACHE_BYTES / 128);
    }

    #[tokio::test]
    async fn reset_and_streaming_writes_cannot_leave_a_shared_cache() {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        txn.set(b"h/c/1/a", b"1").await.unwrap();
        txn.commit().await.unwrap();
        let txn = store.new_txn(true).await.unwrap();
        rows(txn.as_ref()).await;
        let shared = cache(txn.as_ref());
        store.drop_all().await.unwrap();
        assert!(head_entries(txn.as_ref(), 1).await.unwrap().is_none());
        txn.discard();
        let txn = store.new_txn(true).await.unwrap();
        assert!(rows(txn.as_ref()).await.heads.is_empty());
        txn.discard();
        drop(store.streaming_writer(Default::default()));
        assert!(lock(&shared).unwrap().disabled);
        let txn = store.new_txn(true).await.unwrap();
        assert!(head_entries(txn.as_ref(), 1).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn projection_respects_exact_prefixes_and_namespace_layers() {
        use crate::corekv::Reader;
        use crate::namespace::{Namespace, NamespacedTxn};
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        rows(txn.as_ref()).await;
        txn.set(b"h/c/1/a", b"1").await.unwrap();
        txn.set(b"h/c/01/not-in-prefix", b"2").await.unwrap();
        txn.commit().await.unwrap();
        let root = store.new_txn(true).await.unwrap();
        assert!(root
            .collection_head_entries(b"/c/1/", b"/cs/1/")
            .await
            .unwrap()
            .is_none());
        assert!(root
            .collection_head_entries(b"h/c/1/", b"h/cs/2/")
            .await
            .unwrap()
            .is_none());
        assert_eq!(rows(root.as_ref()).await.heads.len(), 1);
        let head = NamespacedTxn::new(root, Namespace::Headstore);
        let view = head
            .collection_head_entries(b"/c/1/", b"/cs/1/")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(view.heads[0].key, b"/c/1/a");
        let nested = NamespacedTxn::new(Box::new(head), Namespace::Headstore);
        assert!(nested
            .collection_head_entries(b"/c/1/", b"/cs/1/")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn reopen_reconstructs_the_same_head_projection() {
        let dir = tempfile::tempdir().unwrap();
        let store = RegolithStore::open(dir.path()).unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        txn.set(b"h/c/1/child", b"2").await.unwrap();
        txn.set(b"h/cs/1/parent/child", b"").await.unwrap();
        txn.commit().await.unwrap();
        let txn = store.new_txn(true).await.unwrap();
        let before = rows(txn.as_ref()).await;
        txn.discard();
        store.close().await.unwrap();
        drop(store);
        let store = RegolithStore::open(dir.path()).unwrap();
        let txn = store.new_txn(true).await.unwrap();
        let after = rows(txn.as_ref()).await;
        assert_eq!(before.heads, after.heads);
        assert_eq!(before.markers, after.markers);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn racing_snapshots_match_native_scans() {
        let store = RegolithStore::in_memory().unwrap();
        let mut tasks = vec![];
        for writer in 0..4 {
            let store = store.clone();
            tasks.push(tokio::spawn(async move {
                for turn in 0..64 {
                    let mut txn = store.new_txn(false).await.unwrap();
                    tokio::task::yield_now().await;
                    let key = format!("h/c/1/{writer}/{turn}");
                    txn.set(key.as_bytes(), b"priority").await.unwrap();
                    if turn > 0 {
                        txn.delete(format!("h/c/1/{writer}/{}", turn - 1).as_bytes())
                            .await
                            .unwrap();
                    }
                    let cached = rows(txn.as_ref()).await;
                    let mut scan = txn
                        .iterator(crate::corekv::IterOptions::new().with_prefix(b"h/c/1/".to_vec()))
                        .await
                        .unwrap();
                    let mut native = vec![];
                    while let Some(row) = scan.next().await.unwrap() {
                        native.push(row);
                    }
                    scan.close().await.unwrap();
                    drop(scan);
                    assert_eq!(cached.heads, native);
                    txn.commit().await.unwrap();
                }
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
    }

    #[tokio::test]
    async fn serializable_writers_keep_native_scan_validation() {
        let dir = tempfile::tempdir().unwrap();
        let store = RegolithStore::open_with_options(
            dir.path(),
            RegolithStoreOptions::memory().with_isolation(regolith::IsolationLevel::Serializable),
        )
        .unwrap();
        let txn = store.new_txn(false).await.unwrap();
        assert!(head_entries(txn.as_ref(), 1).await.unwrap().is_none());
    }
}
