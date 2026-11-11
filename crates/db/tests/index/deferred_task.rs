//! The deferred index drain task settles owed work by itself: after each
//! commit that asks for it, and on a sweep for work nobody asked for
//! (`proofs/tla/DeferredDrainTask.tla`).

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use db::{AutoCommitMutator, DbOptions, DB};
use document::{DocID, Document, FieldOp, NormalValue};
use query::DocMutator;
use rapidhash::RapidHashSet;
use schema::{
    CType, CollectionVersion, FieldDescription, FieldKind, IndexKind, IndexedFieldDescription,
    OrderedIndexDescription,
};
use storage::corekv::{IterOptions, Iterator as _};
use storage::keys::{index_pending_log, IndexDataStoreKey, IndexedField};
use storage::RegolithStore;

const COLLECTION: &str = "Tallies";
const BUSY: &str = "Busy";
const WAIT: Duration = Duration::from_secs(10);
const NO_SWEEP: Duration = Duration::from_secs(3600);

fn collection(name: &str) -> CollectionVersion {
    CollectionVersion::new(
        name,
        format!("{name}-v1"),
        format!("col-deferred-task-{name}"),
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "count", FieldKind::int(), CType::PCounter),
            FieldDescription::new("3", "other", FieldKind::int(), CType::PCounter),
        ],
    )
}

/// Runs a test body on the test pool, where a timer can wait.
fn on_pool<F, Fut>(body: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + 'static,
{
    let done = workers::spawn(Some(crate::common::worker_io::pool()), "test", body).unwrap();
    workers::block_on(done).unwrap().unwrap();
}

async fn pause() {
    workers::sleep(Duration::from_millis(20)).unwrap().await;
}

/// Gives the task time to serve the requests already made, so a later
/// write cannot ride on one of them.
async fn quiet() {
    workers::sleep(Duration::from_millis(300)).unwrap().await;
}

struct Node {
    db: Arc<DB<RegolithStore>>,
    task: workers::Background,
    short_id: u32,
    by_count: u32,
    by_other: u32,
}

impl Node {
    async fn open(sweep: Duration) -> Self {
        let options = DbOptions::default()
            .with_pool(crate::common::worker_io::pool().clone())
            .with_deferred_index_sweep(sweep);
        let db = Arc::new(DB::with_options(RegolithStore::in_memory().unwrap(), options).unwrap());
        db.create_collection(collection(COLLECTION)).await.unwrap();
        db.create_collection(collection(BUSY)).await.unwrap();
        let mut ids = Vec::new();
        for (name, field) in [
            (COLLECTION, "count"),
            (COLLECTION, "other"),
            (BUSY, "count"),
        ] {
            let index = db
                .create_index(
                    name,
                    Some(&format!("by_{field}")),
                    vec![IndexedFieldDescription {
                        name: field.to_string(),
                        descending: false,
                    }],
                    IndexKind::Ordered(OrderedIndexDescription { unique: false }),
                )
                .await
                .unwrap();
            ids.push(index.id);
        }
        let short_id = db
            .require_collection(COLLECTION)
            .unwrap()
            .resolved_root_id();
        let task = db.clone().start_deferred_index_task();
        quiet().await;
        Self {
            db,
            task,
            short_id,
            by_count: ids[0],
            by_other: ids[1],
        }
    }

    async fn create(&self, count: i64) -> DocID {
        self.create_in(COLLECTION, count).await
    }

    async fn create_in(&self, name: &str, count: i64) -> DocID {
        let mut doc = Document::new(self.db.require_collection(name).unwrap().schema_arc());
        doc.set("count", NormalValue::Int(count)).unwrap();
        doc.set("other", NormalValue::Int(count)).unwrap();
        AutoCommitMutator::new(Arc::clone(&self.db))
            .create(name, doc)
            .await
            .unwrap()
            .doc_id
    }

    async fn increment(&self, id: &DocID, fields: &[&str], by: i64) {
        self.increment_in(COLLECTION, id, fields, by).await
    }

    async fn increment_in(&self, name: &str, id: &DocID, fields: &[&str], by: i64) {
        let mut doc = Document::new(self.db.require_collection(name).unwrap().schema_arc());
        doc.set_id(id.clone());
        let mut changed = RapidHashSet::default();
        for field in fields {
            doc.edit(*field, FieldOp::Increment(NormalValue::Int(by)))
                .unwrap();
            changed.insert(field.to_string());
        }
        AutoCommitMutator::new(Arc::clone(&self.db))
            .update(name, doc, changed)
            .await
            .unwrap();
    }

    /// How many stored entries of `index_id` hold `value`.
    async fn stored_at(&self, index_id: u32, value: i64) -> usize {
        let prefix = IndexDataStoreKey::new(
            self.short_id,
            index_id,
            vec![IndexedField::ascending(NormalValue::Int(value))],
        )
        .try_bytes()
        .unwrap();
        self.count_keys(prefix).await
    }

    /// How many entries the pending log of `index_id` still holds.
    async fn owed(&self, index_id: u32) -> usize {
        let log = index_pending_log(self.short_id, index_id);
        self.count_keys(log.entry_prefix().to_vec()).await
    }

    async fn count_keys(&self, prefix: Vec<u8>) -> usize {
        let txn = self.db.new_txn(true).await.unwrap();
        let datastore = txn.datastore().unwrap();
        let mut iter = datastore
            .iterator(IterOptions::new().with_prefix(prefix))
            .await
            .unwrap();
        let mut count = 0;
        while iter.next().await.unwrap().is_some() {
            count += 1;
        }
        iter.close().await.unwrap();
        let _ = txn.discard();
        count
    }

    /// Owes `index_id` an entry without asking for a drain, as a crash
    /// that lost the request would.
    async fn owe_silently(&self, index_id: u32, entry: &[u8]) {
        let txn = self.db.new_txn(false).await.unwrap();
        txn.datastore()
            .unwrap()
            .append(&index_pending_log(self.short_id, index_id), entry, None)
            .await
            .unwrap();
        txn.commit().await.unwrap();
    }

    /// Waits until the stored entries of `index_id` moved from `from` to `to`.
    async fn settles(&self, index_id: u32, from: i64, to: i64) -> bool {
        let start = Instant::now();
        while start.elapsed() < WAIT {
            if (
                self.stored_at(index_id, from).await,
                self.stored_at(index_id, to).await,
            ) == (0, 1)
            {
                return true;
            }
            pause().await;
        }
        false
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[test]
fn a_commit_is_drained_with_no_call_to_drain() {
    on_pool(|| async {
        let node = Node::open(NO_SWEEP).await;
        let id = node.create(1).await;
        node.increment(&id, &["count"], 4).await;

        assert!(
            node.settles(node.by_count, 1, 5).await,
            "the drain task moved the entry once the increment committed"
        );
        assert_eq!(node.owed(node.by_count).await, 0);
    });
}

/// A log entry that is not a document short id fails every drain of its index.
const POISON: &[u8] = &[1, 2, 3];

async fn a_failing_index_does_not_hold_up(poisoned: fn(&Node) -> u32, healthy: fn(&Node) -> u32) {
    let node = Node::open(NO_SWEEP).await;
    let id = node.create(1).await;
    node.owe_silently(poisoned(&node), POISON).await;
    node.increment(&id, &["count", "other"], 4).await;

    assert!(
        node.settles(healthy(&node), 1, 5).await,
        "the healthy index drained past the failing one"
    );
    assert_eq!(node.owed(healthy(&node)).await, 0);
}

#[test]
fn a_failing_first_index_does_not_hold_up_the_second() {
    on_pool(|| a_failing_index_does_not_hold_up(|n| n.by_count, |n| n.by_other));
}

#[test]
fn a_failing_second_index_does_not_hold_up_the_first() {
    on_pool(|| a_failing_index_does_not_hold_up(|n| n.by_other, |n| n.by_count));
}

#[test]
fn the_sweep_runs_while_requests_keep_coming() {
    on_pool(|| async {
        let node = Node::open(Duration::from_millis(200)).await;
        let busy = node.create_in(BUSY, 1).await;
        node.owe_silently(node.by_other, &u64::MAX.to_be_bytes())
            .await;

        let start = Instant::now();
        let mut swept = false;
        while !swept && start.elapsed() < WAIT {
            node.increment_in(BUSY, &busy, &["count"], 1).await;
            swept = node.owed(node.by_other).await == 0;
            pause().await;
        }
        assert!(
            swept,
            "the sweep drained work no request named while another index was busy"
        );
    });
}

#[test]
fn a_purge_is_drained_with_no_call_to_drain() {
    on_pool(|| async {
        let node = Node::open(NO_SWEEP).await;
        let id = node.create(1).await;
        quiet().await;

        assert_eq!(
            node.db
                .purge_by_doc_ids(COLLECTION, &[id], None)
                .await
                .unwrap(),
            1
        );

        let start = Instant::now();
        while node.stored_at(node.by_count, 1).await > 0 && start.elapsed() < WAIT {
            pause().await;
        }
        assert_eq!(
            node.stored_at(node.by_count, 1).await,
            0,
            "the purge's entry was drained"
        );
        assert_eq!(node.owed(node.by_count).await, 0);
    });
}
