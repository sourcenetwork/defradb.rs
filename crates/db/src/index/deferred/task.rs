//! When deferred indexes drain: at open, on each commit that owes one, and on
//! a sweep for work nobody asked for, such as a request a crash lost
//! (`proofs/tla/DeferredDrainTask.tla`).

use std::future::Future;
use std::pin::Pin;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use futures::future::{select, Either};
use rapidhash::{HashSetExt, RapidHashMap, RapidHashSet};
use storage::corekv::Store;

use super::drain::drain_index;
use super::{DrainRequests, IndexRef};
use crate::database::DB;
use crate::index::error::{Error, Result};

/// How many plans an index drains in one pass before the others get a turn.
const TURN: usize = 4;

/// What one turn of an index did.
enum Turn {
    Idle,
    Drained,
    More,
}

/// Why the drain task woke.
enum Wake {
    Sweep,
    Request,
}

/// What one turn of an index did with the documents it planned.
struct TurnReport {
    turn: Turn,
    moved: Vec<u64>,
    failed: Vec<(u64, Error)>,
    carried: bool,
}

/// Failed drains in a row per index, and per document a drain carried
/// forward, kept by the drain task.
#[derive(Default)]
struct Failures {
    indexes: RapidHashMap<IndexRef, u32>,
    docs: RapidHashMap<IndexRef, RapidHashMap<u64, u32>>,
}

impl Failures {
    fn has(&self, target: IndexRef) -> bool {
        self.indexes.contains_key(&target)
    }

    /// Warns on the first failure and then each time the count doubles, so
    /// a stuck index does not flood the log.
    fn failed(&mut self, name: &str, target: IndexRef, error: &Error) {
        let count = self.indexes.entry(target).or_insert(0);
        *count += 1;
        if count.is_power_of_two() {
            tracing::warn!(
                collection = %name,
                collection_short_id = target.0,
                index_id = target.1,
                failures = *count,
                %error,
                "deferred index drain failed; the next sweep retries it"
            );
        }
    }

    fn drained(&mut self, name: &str, target: IndexRef) {
        if let Some(count) = self.indexes.remove(&target) {
            tracing::info!(
                collection = %name,
                index_id = target.1,
                failures = count,
                "deferred index drains again"
            );
        }
    }

    /// The documents a request pass leaves to the sweep. A sweep tries them all.
    fn stuck(&self, target: IndexRef, sweep: bool) -> RapidHashSet<u64> {
        match self.docs.get(&target) {
            Some(docs) if !sweep => docs.keys().copied().collect(),
            _ => RapidHashSet::new(),
        }
    }

    /// Notes what a turn did with each document, warning as `failed` does.
    fn turned(&mut self, name: &str, target: IndexRef, report: &TurnReport) {
        let docs = self.docs.entry(target).or_default();
        for doc in &report.moved {
            if let Some(count) = docs.remove(doc) {
                tracing::info!(
                    collection = %name,
                    index_id = target.1,
                    doc_short_id = doc,
                    failures = count,
                    "deferred index drains the document again"
                );
            }
        }
        for (doc, error) in &report.failed {
            let count = docs.entry(*doc).or_insert(0);
            *count += 1;
            if count.is_power_of_two() {
                tracing::warn!(
                    collection = %name,
                    collection_short_id = target.0,
                    index_id = target.1,
                    failures = *count,
                    %error,
                    "deferred index drain left a document owed; the next sweep retries it"
                );
            }
        }
        if docs.is_empty() {
            self.docs.remove(&target);
        }
    }
}

/// Waits for the next pass. A due sweep goes first, so steady requests
/// cannot hold it off.
async fn next_wake<F>(requests: &DrainRequests, sweep: Pin<&mut F>) -> Wake
where
    F: Future<Output = ()>,
{
    match select(sweep, std::pin::pin!(requests.requested())).await {
        Either::Left(_) => Wake::Sweep,
        Either::Right(_) => Wake::Request,
    }
}

impl<S> DB<S>
where
    S: Store + 'static,
{
    /// Drains every deferred index until nothing is owed but the documents
    /// that fail, which stay owed. Each index drains on its own, so one that
    /// fails does not stop the others; the first failure is returned.
    pub async fn drain_deferred_indexes(&self) -> Result<()> {
        let mut failure = None;
        for (name, target) in self.deferred_indexes()? {
            match self
                .drain_turn(&name, target, usize::MAX, RapidHashSet::new())
                .await
            {
                Ok(report) => {
                    if let Some((_, source)) = report.failed.into_iter().next() {
                        failure.get_or_insert(Error::DeferredDrain {
                            collection: name,
                            index_id: target.1,
                            source: Box::new(source),
                        });
                    }
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    /// Every deferred index, with its collection's name.
    fn deferred_indexes(&self) -> Result<Vec<(String, IndexRef)>> {
        let mut indexes = Vec::new();
        for name in self
            .list_collections()
            .map_err(|e| Error::Other(e.to_string()))?
        {
            let Some(collection) = self
                .get_collection(&name)
                .map_err(|e| Error::Other(e.to_string()))?
            else {
                continue;
            };
            let short_id = collection.resolved_root_id();
            for index in collection.write_indexes() {
                if crdt::deferred_index(index, collection.schema(), collection.crdts()) {
                    indexes.push((name.clone(), (short_id, index.id)));
                }
            }
        }
        Ok(indexes)
    }

    /// Drains up to `plans` plans of one index, carrying forward the
    /// documents in `skip` and those that fail. It stops at a plan that frees
    /// nothing, so a document that keeps failing waits for the next sweep.
    async fn drain_turn(
        &self,
        name: &str,
        target: IndexRef,
        plans: usize,
        mut skip: RapidHashSet<u64>,
    ) -> Result<TurnReport> {
        let mut report = TurnReport {
            turn: Turn::Idle,
            moved: Vec::new(),
            failed: Vec::new(),
            carried: false,
        };
        for _ in 0..plans {
            let drained = drain_index(self, name, target.1, &skip)
                .await
                .map_err(|source| Error::DeferredDrain {
                    collection: name.to_string(),
                    index_id: target.1,
                    source: Box::new(source),
                })?;
            let Some(drained) = drained else {
                return Ok(report);
            };
            report.carried |= drained.carried;
            skip.extend(drained.failed.iter().map(|(doc, _)| *doc));
            report.moved.extend(drained.moved);
            report.failed.extend(drained.failed);
            if !drained.progressed {
                return Ok(report);
            }
            report.turn = Turn::Drained;
        }
        report.turn = Turn::More;
        Ok(report)
    }

    /// One pass of the drain task: the indexes asked for, or every index on
    /// a sweep. Returns how many indexes had work.
    async fn drain_pass(&self, failures: &mut Failures, sweep: bool) -> usize {
        let requested = self.deferred_drains.take();
        let indexes = match self.deferred_indexes() {
            Ok(indexes) => indexes,
            Err(error) => {
                tracing::warn!(%error, "deferred index drain could not list the indexes; the next sweep retries");
                for target in requested {
                    self.deferred_drains.defer(target);
                }
                return 0;
            }
        };
        let mut worked = 0;
        for (name, target) in indexes {
            if !sweep && !requested.contains(&target) {
                continue;
            }
            // A failed index waits for the sweep, so it does not fail again on every request.
            if !sweep && failures.has(target) {
                self.deferred_drains.defer(target);
                continue;
            }
            let stuck = failures.stuck(target, sweep);
            match self.drain_turn(&name, target, TURN, stuck).await {
                Ok(report) => {
                    failures.drained(&name, target);
                    failures.turned(&name, target, &report);
                    match report.turn {
                        Turn::Idle => {}
                        Turn::Drained => worked += 1,
                        Turn::More => {
                            worked += 1;
                            self.deferred_drains.request(target);
                        }
                    }
                    // Owed again, kept for the sweep without a wake.
                    if report.carried && !matches!(report.turn, Turn::More) {
                        self.deferred_drains.defer(target);
                    }
                }
                Err(error) => {
                    failures.failed(&name, target, &error);
                    self.deferred_drains.defer(target);
                }
            }
        }
        worked
    }

    /// The first pass after open is a sweep: requests made before a crash are gone.
    async fn settle(&self, failures: &mut Failures) {
        let indexes = self.drain_pass(failures, true).await;
        if indexes > 0 {
            tracing::info!(indexes, "deferred indexes owed work at open; draining it");
        }
    }

    /// The drain on wasm, run on the page's event loop because the database
    /// is not `Send` there. Holding it only while draining lets it close.
    /// `sleep` is the page's timer, which the sweep waits on.
    #[cfg(target_arch = "wasm32")]
    pub async fn run_deferred_index_drain<T, F>(db: std::sync::Weak<Self>, mut sleep: T)
    where
        T: FnMut(std::time::Duration) -> F,
        F: Future<Output = ()>,
    {
        let Some((requests, every)) = db.upgrade().map(|db| {
            (
                db.deferred_drains.clone(),
                db.options().deferred_index_sweep(),
            )
        }) else {
            return;
        };
        let mut failures = Failures::default();
        match db.upgrade() {
            Some(db) => db.settle(&mut failures).await,
            None => return,
        }
        loop {
            let mut sweep = std::pin::pin!(sleep(every));
            loop {
                let wake = next_wake(&requests, sweep.as_mut()).await;
                let Some(db) = db.upgrade() else {
                    return;
                };
                match wake {
                    Wake::Request => {
                        db.drain_pass(&mut failures, false).await;
                    }
                    Wake::Sweep => {
                        db.drain_pass(&mut failures, true).await;
                        break;
                    }
                }
            }
        }
    }

    /// Starts the deferred index drain on the database's pool: everything
    /// owed at open first, then each request, and a sweep on a fixed deadline.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_deferred_index_task(self: Arc<Self>) -> workers::Background {
        let pool = self.pool().cloned();
        workers::spawn_background(pool.as_ref(), "db::deferred_index", move || async move {
            let every = self.options().deferred_index_sweep();
            let mut failures = Failures::default();
            self.settle(&mut failures).await;
            loop {
                let Ok(sweep) = workers::sleep(every) else {
                    tracing::error!(
                        task = "db::deferred_index",
                        "the deferred index drain stopped off a worker; owed entries settle at the next open"
                    );
                    return;
                };
                let mut sweep = std::pin::pin!(sweep);
                loop {
                    match next_wake(&self.deferred_drains, sweep.as_mut()).await {
                        Wake::Request => {
                            self.drain_pass(&mut failures, false).await;
                        }
                        Wake::Sweep => {
                            self.drain_pass(&mut failures, true).await;
                            break;
                        }
                    }
                }
            }
        })
    }
}
