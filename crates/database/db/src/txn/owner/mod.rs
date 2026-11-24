//! Explicit transactions, each owned by one worker for its whole life.
//!
//! A transaction is begun on a worker of the pool and stays there: one task
//! on that worker (its actor, `actor`) holds it by value and is the only code
//! that ever touches it. Its ID names the worker (`id`), so a request for it
//! goes from the edge straight to that worker with `Target::Worker`, one hop
//! in and one out, and no shared lookup. On the worker the request finds the
//! actor's mailbox in the worker's own table (`table`) and queues there: the
//! mailbox order is the transaction's order, so requests run one at a time,
//! a commit or discard runs after every request queued before it, and the
//! idle sweep's ask (`expire_idle`) ends a transaction only when it is the
//! next message and nothing admitted has run for the idle limit, so a request
//! that arrived first keeps its transaction.
//!
//! Nothing here knows what a transaction is: the registry gives the value
//! (`T`), how a request is admitted (its caller owns the transaction), what a
//! request does with it, and what ending it means. Dropping the value is how
//! a transaction ends without a commit (abandoned, swept, the registry or the
//! pool gone), so its holder's `Drop` discards it, every time.

mod actor;
mod id;
mod idle;
mod table;
#[cfg(test)]
mod tests;

use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use defra_core::thread_bounds::MaybeSend;
use futures::future::LocalBoxFuture;
use rand::random;
use rapidhash::fast::RandomState;
use regolith::sync::{unbounded, ArrayQueue, HopscotchMap};
use web_time::Instant;
use workers::Target;
#[cfg(not(target_family = "wasm"))]
use workers::{Pool, Submit};

use actor::Message;
use table::{Entry, Table};

use crate::txn::fence::{CommitFences, Fence, Landing};

pub(crate) use id::TxnId;

/// Why a request could not reach its transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unavailable {
    /// No such transaction on its owner: it never existed, it ended, or the
    /// caller does not own it.
    NotFound,
    /// The worker that began the transaction cannot be named in its ID.
    Unroutable,
    /// The pool could not run the request.
    Pool(workers::Error),
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unavailable::NotFound => f.write_str("transaction not found"),
            Unavailable::Unroutable => {
                f.write_str("the worker cannot be named in a transaction ID")
            }
            Unavailable::Pool(error) => write!(f, "{error}"),
        }
    }
}

impl From<workers::Error> for Unavailable {
    fn from(error: workers::Error) -> Self {
        Unavailable::Pool(error)
    }
}

/// The outcome a client may ask for after it lost a commit's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The commit was queued but has not landed yet.
    Committing,
    /// The transaction is open and no commit was queued.
    Open,
    /// The commit landed.
    Committed,
    /// The transaction ended without a commit.
    Aborted,
    /// The commit failed while the engine made it durable: it may have
    /// landed, and only a read after a restart tells.
    Indeterminate,
}

impl Outcome {
    fn is_final(self) -> bool {
        matches!(
            self,
            Outcome::Committed | Outcome::Aborted | Outcome::Indeterminate
        )
    }

    /// The outcome a finished commit attempt leaves: committed, aborted, or
    /// indeterminate when its error says the commit may have landed.
    pub(crate) fn of<T>(result: &Result<T, query::TransactionError>) -> Self {
        match result {
            Ok(_) => Outcome::Committed,
            Err(error) => match error.commit_outcome() {
                Some(defra_core::CommitOutcome::Indeterminate) => Outcome::Indeterminate,
                _ => Outcome::Aborted,
            },
        }
    }

    pub(crate) fn store_committing_if_open(status: &AtomicU8) {
        let _ = status.compare_exchange(
            Outcome::Open as u8,
            Outcome::Committing as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub(crate) fn store(self, status: &AtomicU8) {
        status.store(self as u8, Ordering::Release);
    }
}

impl TryFrom<u8> for Outcome {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Outcome::Committing),
            1 => Ok(Outcome::Open),
            2 => Ok(Outcome::Committed),
            3 => Ok(Outcome::Aborted),
            4 => Ok(Outcome::Indeterminate),
            _ => Err(value),
        }
    }
}

/// Every worker's table, shared by the registry and the requests on their
/// way; actors hold it weakly, so dropping the registry ends every
/// transaction it owns.
pub(super) struct Shared<T, K>
where
    T: 'static,
    K: 'static,
{
    tables: Box<[Table<T, K>]>,
    outcomes: Box<[OutcomeStore<K>]>,
}

/// What one transaction's ID last said, and when the record may go.
#[derive(Clone)]
struct OutcomeRecord<K> {
    status: Arc<AtomicU8>,
    owner: K,
    finished: Instant,
}

const OUTCOME_CAP: usize = 4096;

/// One worker's retained outcomes, keyed by the transaction's full ID.
struct OutcomeStore<K>
where
    K: 'static,
{
    records: HopscotchMap<(u64, u64), OutcomeRecord<K>, RandomState>,
    retention_nanos: AtomicU64,
    evictions: AtomicU64,
    /// The size at which an insert scans for final results to drop.
    next_evict: AtomicUsize,
    cap: usize,
}

impl<K> OutcomeStore<K>
where
    K: Clone + PartialEq + Send + Sync + 'static,
{
    fn new(retention: Duration, cap: usize) -> Self {
        Self {
            records: HopscotchMap::with_hasher(RandomState::default()),
            retention_nanos: AtomicU64::new(retention.as_nanos() as u64),
            evictions: AtomicU64::new(0),
            next_evict: AtomicUsize::new(cap),
            cap,
        }
    }

    fn status(&self, id: TxnId) -> Option<Outcome> {
        self.records
            .get(&id.key())
            .and_then(|record| Outcome::try_from(record.status.load(Ordering::Acquire)).ok())
    }

    /// At the cap the oldest final results make room. A live transaction's
    /// record is never evicted: there is one per open transaction, as there is
    /// a table entry, and its result is still to come.
    fn insert(&self, id: TxnId, record: OutcomeRecord<K>) {
        if self.records.len() >= self.next_evict.load(Ordering::Relaxed) {
            self.evict();
        }
        self.records.insert(id.key(), record);
    }

    /// Drops the oldest final results, in one scan, until the store is an
    /// eighth below the cap or holds only live records. The next scan waits
    /// for at least that many more inserts, so a store full of live records
    /// is not rescanned on every begin.
    fn evict(&self) {
        let batch = (self.cap / 8).max(1);
        let mut live = 0;
        let mut finals: Vec<(Instant, (u64, u64))> = self
            .records
            .iter()
            .filter_map(|(key, candidate)| {
                let status = Outcome::try_from(candidate.status.load(Ordering::Acquire))
                    .unwrap_or(Outcome::Open);
                if !status.is_final() {
                    live += 1;
                    return None;
                }
                Some((candidate.finished, key))
            })
            .collect();
        let keep = self.cap.saturating_sub(batch).saturating_sub(live);
        let excess = finals.len().saturating_sub(keep);
        if excess > 0 && excess < finals.len() {
            finals.select_nth_unstable_by_key(excess - 1, |(finished, _)| *finished);
        }
        finals.truncate(excess);
        for (_, key) in &finals {
            self.records.remove(key);
        }
        self.next_evict
            .store(self.cap.max(live + batch), Ordering::Relaxed);
        if finals.is_empty() {
            return;
        }
        let total = self
            .evictions
            .fetch_add(finals.len() as u64, Ordering::Relaxed)
            + finals.len() as u64;
        tracing::warn!(
            evicted = finals.len(),
            live,
            total,
            cap = self.cap,
            "transaction outcome store is full; the oldest final outcomes were dropped before their retention ended"
        );
    }

    fn remove(&self, id: TxnId) {
        self.records.remove(&id.key());
    }

    fn mark(&self, id: TxnId, status: Outcome, now: Instant) {
        let Some(record) = self.records.get(&id.key()) else {
            return;
        };
        record.status.store(status as u8, Ordering::Release);
        self.records.insert(
            id.key(),
            OutcomeRecord {
                status: Arc::clone(&record.status),
                owner: record.owner.clone(),
                finished: now,
            },
        );
    }

    fn touch(&self, id: TxnId, now: Instant) {
        let Some(record) = self.records.get(&id.key()) else {
            return;
        };
        self.records.insert(
            id.key(),
            OutcomeRecord {
                status: Arc::clone(&record.status),
                owner: record.owner.clone(),
                finished: now,
            },
        );
    }

    fn admitted(&self, id: TxnId, key: &K) -> bool {
        self.records
            .get(&id.key())
            .is_some_and(|record| record.owner == *key)
    }

    fn cell(&self, id: TxnId) -> Option<Arc<AtomicU8>> {
        self.records
            .get(&id.key())
            .map(|record| Arc::clone(&record.status))
    }

    fn expire(&self, now: Instant) {
        let retention = Duration::from_nanos(self.retention_nanos.load(Ordering::Relaxed));
        if retention.is_zero() {
            return;
        }
        let expired: Vec<(u64, u64)> = self
            .records
            .iter()
            .filter_map(|(key, record)| {
                let status = Outcome::try_from(record.status.load(Ordering::Acquire))
                    .unwrap_or(Outcome::Open);
                (now.duration_since(record.finished) >= retention && status.is_final())
                    .then_some(key)
            })
            .collect();
        for key in expired {
            self.records.remove(&key);
        }
    }

    fn set_retention(&self, retention: Duration) {
        self.retention_nanos
            .store(retention.as_nanos() as u64, Ordering::Relaxed);
    }

    #[cfg(test)]
    fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }
}

/// The transactions of one registry, spread over the pool's workers. Each is
/// kept with the key of the caller that may use it (`K`): a caller with
/// another key is told the transaction does not exist, before anything is
/// sent to its owner.
///
/// In a browser the page's thread is the one worker: every call already runs
/// there, so the work runs in place, and nothing needs to be `Send`.
pub(crate) struct Owners<T, K>
where
    T: MaybeSend + 'static,
    K: Clone + PartialEq + Send + Sync + 'static,
{
    #[cfg(not(target_family = "wasm"))]
    pool: Pool,
    start_token: u64,
    shared: Arc<Shared<T, K>>,
    /// Where a commit whose caller stops waiting is fenced until it lands.
    fences: Arc<CommitFences>,
}

impl<T, K> Owners<T, K>
where
    T: MaybeSend + 'static,
    K: Clone + PartialEq + Send + Sync + 'static,
{
    /// The transactions of a registry on the workers of `pool`, their
    /// commits fenced in `fences` when a caller stops waiting.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn fenced(pool: Pool, fences: Arc<CommitFences>) -> Self {
        let tables = (0..pool.workers())
            .map(|_| Table::new())
            .collect::<Box<_>>();
        let outcomes = (0..pool.workers())
            .map(|_| OutcomeStore::new(Duration::from_secs(600), OUTCOME_CAP))
            .collect();
        Self {
            pool,
            start_token: TxnId::start_token(random()),
            shared: Arc::new(Shared { tables, outcomes }),
            fences,
        }
    }

    /// The transactions of a registry on the workers of `pool`, with fences
    /// of their own.
    #[cfg(all(test, not(target_family = "wasm")))]
    pub(crate) fn new(pool: Pool) -> Self {
        Self::fenced(pool, Arc::default())
    }

    /// The transactions of a registry on the page's one worker.
    #[cfg(target_family = "wasm")]
    pub(crate) fn here(fences: Arc<CommitFences>) -> Self {
        Self {
            start_token: TxnId::start_token(random()),
            shared: Arc::new(Shared {
                tables: Box::new([Table::new()]),
                outcomes: Box::new([OutcomeStore::new(Duration::from_secs(600), OUTCOME_CAP)]),
            }),
            fences,
        }
    }

    /// How many workers own transactions.
    fn workers(&self) -> usize {
        self.shared.tables.len()
    }

    fn table(&self, id: TxnId) -> &Table<T, K> {
        &self.shared.tables[id.owner(self.workers())]
    }

    /// Runs the work `make` builds on `target`, a worker of the pool, and
    /// gives back its output.
    #[cfg(not(target_family = "wasm"))]
    async fn place<F, Fut>(
        &self,
        target: Target,
        name: &'static str,
        make: F,
    ) -> Result<Fut::Output, Unavailable>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let placed = self.pool.submit_with(target, named(name), make).await?;
        Ok(placed.await?)
    }

    /// The page's one worker is the calling thread: the work runs in place.
    #[cfg(target_family = "wasm")]
    async fn place<F, Fut>(
        &self,
        _target: Target,
        _name: &'static str,
        make: F,
    ) -> Result<Fut::Output, Unavailable>
    where
        F: FnOnce() -> Fut + 'static,
        Fut: Future + 'static,
    {
        Ok(make().await)
    }

    /// Runs the work `make` builds outside every transaction (a standalone
    /// read) and gives back its output: in place when the caller is already
    /// a worker, otherwise on the least loaded worker.
    pub(crate) async fn run_anywhere<F, Fut>(
        &self,
        name: &'static str,
        make: F,
    ) -> Result<Fut::Output, Unavailable>
    where
        F: FnOnce() -> Fut + MaybeSend + 'static,
        Fut: Future + 'static,
        Fut::Output: MaybeSend + 'static,
    {
        if workers::on_worker() {
            return Ok(make().await);
        }
        self.place(Target::LeastLoaded, name, make).await
    }

    /// Opens a transaction with `open`, given its ID, on the least loaded
    /// worker, which owns it from then on for the caller `key` names, and
    /// returns the ID. An error from `open` comes back as is. A caller that
    /// stops waiting leaves a transaction nobody knows the ID of; the idle
    /// sweep ends it.
    pub(crate) async fn begin<O, Fut, E>(
        &self,
        key: K,
        open: O,
    ) -> Result<Result<TxnId, E>, Unavailable>
    where
        O: FnOnce(TxnId) -> Fut + MaybeSend + 'static,
        Fut: Future<Output = Result<T, E>> + 'static,
        E: MaybeSend + 'static,
    {
        let shared = Arc::downgrade(&self.shared);
        let workers = self.workers();
        let start_token = self.start_token;
        self.place(Target::LeastLoaded, "txn.begin", move || async move {
            let worker = here()?;
            let sequence = match shared.upgrade() {
                Some(tables) => tables.tables[worker].draw(),
                None => return Err(Unavailable::NotFound),
            };
            let id = TxnId::new(sequence, worker, workers, start_token)
                .ok_or(Unavailable::Unroutable)?;
            let txn = match open(id).await {
                Ok(txn) => txn,
                Err(error) => return Ok(Err(error)),
            };
            let (mailbox, inbox) = unbounded();
            let idle = Arc::new(ArrayQueue::new(1));
            let status = Arc::new(AtomicU8::new(Outcome::Open as u8));
            let outcome = OutcomeRecord {
                status: Arc::clone(&status),
                owner: key.clone(),
                finished: Instant::now(),
            };
            // The slot is new and holds one: the push cannot find it full.
            let _ = idle.push(txn);
            let Some(tables) = shared.upgrade() else {
                return Err(Unavailable::NotFound);
            };
            tables.tables[worker].insert(
                id,
                Entry {
                    mailbox,
                    idle: Arc::clone(&idle),
                    key,
                    ending: false,
                },
            );
            tables.outcomes[worker].insert(id, outcome);
            drop(tables);
            if let Err(error) = start(actor::own(idle, id, worker, inbox, shared.clone(), status)) {
                if let Some(tables) = shared.upgrade() {
                    tables.tables[worker].take(id);
                    tables.outcomes[worker].remove(id);
                }
                return Err(Unavailable::Pool(error));
            }
            Ok(Ok(id))
        })
        .await?
    }

    /// Runs `work` on transaction `id`, on its owner, after every request for
    /// it that arrived earlier and before every one that arrives later. A
    /// caller whose `key` does not own it gets [`Unavailable::NotFound`], as
    /// for a transaction that does not exist.
    pub(crate) async fn run<R, W>(&self, id: TxnId, key: &K, work: W) -> Result<R, Unavailable>
    where
        R: MaybeSend + 'static,
        W: for<'t> FnOnce(&'t mut T) -> LocalBoxFuture<'t, R> + MaybeSend + 'static,
    {
        // Early, for a caller turned away without a hop; `send` decides.
        if !self.table(id).runs(id, key) {
            return Err(Unavailable::NotFound);
        }
        self.send(id, key, "txn.request", move |reply| {
            Message::Run(actor::work(move |mut txn, idle| {
                Box::pin(async move {
                    let answer = work(&mut txn).await;
                    // Idle again before the answer goes out, so a caller
                    // that has it finds no request running (`finish`).
                    let _ = idle.push(txn);
                    let _ = reply.try_send(answer);
                })
            }))
        })
        .await
    }

    /// Ends transaction `id` with `last`, which gets it by value, after every
    /// request for it that arrived earlier. The transaction leaves the table
    /// before this first waits, so requests that arrive later, and a racing
    /// end, find no transaction. A caller whose `key` does not own it is told
    /// it does not exist, and the transaction stays. The end begins when it
    /// is handed over, unless a request is running the transaction then; it
    /// begins once its owner takes it, after that request. A caller that
    /// stops waiting before the end begins abandons the transaction instead:
    /// it ends unfinished. One that stops waiting later cannot undo the end:
    /// it fences it, so every transaction and read that begins afterwards
    /// waits for it to land.
    pub(crate) async fn finish<R, L>(&self, id: TxnId, key: &K, last: L) -> Result<R, Unavailable>
    where
        R: MaybeSend + 'static,
        L: FnOnce(T) -> LocalBoxFuture<'static, R> + MaybeSend + 'static,
    {
        let table = self.table(id);
        if !table.admits(id, key) {
            return Err(Unavailable::NotFound);
        }
        let entry = table.take(id).ok_or(Unavailable::NotFound)?;
        // A request puts the transaction back before it answers, so the slot
        // is empty only while a request this end waits for is still running.
        let begun = !entry.idle.is_empty();
        let fence = Fence::new(&self.fences);
        let fence_on_drop = FenceOnDrop(Some(Arc::clone(&fence)));
        let (reply, answer) = unbounded();
        let queued = entry.mailbox.try_send(Message::Finish {
            last: Box::new(move |txn| {
                Box::pin(async move {
                    let _ = reply.try_send(last(txn).await);
                })
            }),
            fence: Landing::new(fence),
            begun,
        });
        // Not kept while waiting: a message its owner never takes is then
        // dropped with the channel, and its reply and fence with it.
        drop(entry);
        let ended = match queued {
            Ok(()) => answer.recv_async().await.ok_or(Unavailable::NotFound),
            Err(_) => Err(Unavailable::NotFound),
        };
        fence_on_drop.disarm();
        ended
    }

    /// [`finish`](Self::finish) for a terminal commit whose outcome must not
    /// depend on the caller's wait: the commit begins even when its caller
    /// leaves first, while later transactions stay fenced until it lands.
    pub(crate) async fn finish_detached<R, L>(
        &self,
        id: TxnId,
        key: &K,
        last: L,
    ) -> Result<R, Unavailable>
    where
        R: MaybeSend + 'static,
        L: FnOnce(T) -> LocalBoxFuture<'static, R> + MaybeSend + 'static,
    {
        let table = self.table(id);
        if !table.admits(id, key) {
            return Err(Unavailable::NotFound);
        }
        let entry = table.take(id).ok_or(Unavailable::NotFound)?;
        let fence = Fence::new(&self.fences);
        let fence_on_drop = FenceOnDrop(Some(Arc::clone(&fence)));
        let (reply, answer) = unbounded();
        let queued = entry.mailbox.try_send(Message::Finish {
            last: Box::new(move |txn| {
                Box::pin(async move {
                    let _ = reply.try_send(last(txn).await);
                })
            }),
            fence: Landing::new(fence),
            begun: true,
        });
        drop(entry);
        let ended = match queued {
            Ok(()) => answer.recv_async().await.ok_or(Unavailable::NotFound),
            Err(_) => Err(Unavailable::NotFound),
        };
        fence_on_drop.disarm();
        ended
    }

    /// [`finish`](Self::finish), where `last` may decide on the transaction
    /// not to end it yet ([`Ending::Kept`]): it is put back on its owner for
    /// the same caller, under the same ID, for another end only. The end has
    /// begun, so a request, now as while it was out of the table, finds no
    /// transaction; a commit or a discard finds it, and an abandon or the idle
    /// sweep ends it. Nothing is fenced for a transaction that was kept.
    pub(crate) async fn finish_unless<R, L>(
        &self,
        id: TxnId,
        key: &K,
        last: L,
    ) -> Result<R, Unavailable>
    where
        R: MaybeSend + 'static,
        L: FnOnce(T) -> LocalBoxFuture<'static, Ending<T, R>> + MaybeSend + 'static,
    {
        let shared = Arc::downgrade(&self.shared);
        let keeper = key.clone();
        self.finish(id, key, move |txn| {
            Box::pin(async move {
                match last(txn).await {
                    Ending::Ended(answer) => answer,
                    Ending::Kept(txn, answer) => {
                        reinstate(&shared, id, keeper, txn);
                        answer
                    }
                }
            })
        })
        .await
    }

    /// Lets go of transaction `id` without finishing it, at once and without
    /// waiting: it leaves its owner's table, so no later request finds it and
    /// it no longer counts, and its owner ends it unfinished after the
    /// requests already queued for it. Safe from any thread and with no
    /// runtime (a guard's destructor).
    pub(crate) fn abandon(&self, id: TxnId) {
        self.table(id).abandon(id);
    }

    /// Ends every transaction no request has used for at least `max_idle`,
    /// each after the requests queued before this sweep reached it, and says
    /// how many ended.
    pub(crate) async fn expire_idle(&self, max_idle: Duration) -> Result<usize, Unavailable> {
        let sweeps = (0..self.workers()).map(|worker| {
            let shared = Arc::downgrade(&self.shared);
            self.place(Target::Worker(worker), "txn.sweep", move || async move {
                let Some(shared) = shared.upgrade() else {
                    return 0;
                };
                let (reply, answers) = unbounded();
                let asked = shared.tables[worker].send_all(|| Message::Expire {
                    max_idle,
                    reply: Some(reply.clone()),
                });
                drop(reply);
                shared.outcomes[worker].set_retention(max_idle);
                shared.outcomes[worker].expire(Instant::now());
                drop(shared);
                let mut ended = 0;
                for _ in 0..asked {
                    match answers.recv_async().await {
                        Some(true) => ended += 1,
                        Some(false) => {}
                        None => break,
                    }
                }
                ended
            })
        });
        let mut ended = 0;
        for swept in futures::future::join_all(sweeps).await {
            ended += swept?;
        }
        Ok(ended)
    }

    /// How many transactions are open now, on every worker.
    pub(crate) fn len(&self) -> usize {
        self.shared.tables.iter().map(Table::len).sum()
    }

    /// The outcome `id` names for `key`: the live transaction's state, or its
    /// retained result. A stranger and an ID from another start find nothing.
    pub(crate) fn outcome(&self, id: TxnId, key: &K) -> Option<Outcome> {
        let owner = id.owner(self.workers());
        if self.shared.tables[owner].admits(id, key) {
            return self.shared.outcomes[owner]
                .status(id)
                .or(Some(Outcome::Open));
        }
        if self.shared.outcomes[owner].admitted(id, key) {
            return self.shared.outcomes[owner].status(id);
        }
        None
    }

    /// The status cell a commit or discard updates while ending `id` for
    /// `key`; none if the transaction is gone or another caller owns it.
    pub(crate) fn outcome_status(&self, id: TxnId, key: &K) -> Option<Arc<AtomicU8>> {
        let owner = id.owner(self.workers());
        if !self.shared.tables[owner].admits(id, key) {
            return None;
        }
        self.shared.outcomes[owner]
            .records
            .get(&id.key())
            .map(|record| Arc::clone(&record.status))
    }

    /// Queues the request `build` makes for transaction `id` of the caller
    /// `key` on its owner and waits for the reply the message carries back.
    /// Admission is decided again on the owner, by the lookup that picks the
    /// mailbox ([`Table::send_request`]): the check at the caller only turns
    /// a stranger away early. No reply (the message was dropped, or never
    /// queued) means no such transaction.
    async fn send<R, B>(
        &self,
        id: TxnId,
        key: &K,
        name: &'static str,
        build: B,
    ) -> Result<R, Unavailable>
    where
        R: MaybeSend + 'static,
        B: FnOnce(regolith::sync::UnboundedSender<R>) -> Message<T> + MaybeSend + 'static,
    {
        let shared = Arc::downgrade(&self.shared);
        let owner = id.owner(self.workers());
        let key = key.clone();
        self.place(Target::Worker(owner), name, move || {
            let (reply, answer) = unbounded();
            let queued = shared.upgrade().is_some_and(|shared| {
                shared.tables[owner]
                    .send_request(id, &key, build(reply))
                    .is_ok()
            });
            async move {
                match queued {
                    true => answer.recv_async().await,
                    false => None,
                }
            }
        })
        .await?
        .ok_or(Unavailable::NotFound)
    }
}

impl<T, K> Drop for Owners<T, K>
where
    T: MaybeSend + 'static,
    K: Clone + PartialEq + Send + Sync + 'static,
{
    /// A registry that goes ends every transaction it owns.
    fn drop(&mut self) {
        self.shared.tables.iter().for_each(Table::abandon_all);
    }
}

/// While a finish waits: a caller that stops waiting fences the end it handed
/// over until the owner is done with it, and abandons it if it had not begun.
struct FenceOnDrop(Option<Arc<Fence>>);

impl FenceOnDrop {
    /// The finish got its answer: nothing is fenced.
    fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for FenceOnDrop {
    fn drop(&mut self) {
        if let Some(fence) = self.0.take() {
            fence.abandon();
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn named(name: &'static str) -> Submit {
    Submit {
        name: Some(name),
        bytes: None,
    }
}

/// The index of the calling worker, which owns what it begins.
#[cfg(not(target_family = "wasm"))]
fn here() -> Result<usize, Unavailable> {
    workers::current_worker()
        .map(|worker| worker.index)
        .ok_or(Unavailable::Pool(workers::Error::NotOnWorker))
}

/// How an end that may keep its transaction came out.
pub(crate) enum Ending<T, R> {
    /// The transaction ended, with this answer.
    Ended(R),
    /// The transaction was not ended: it goes back to its owner, and the
    /// caller gets the answer.
    Kept(T, R),
}

/// Puts `txn` back on the calling worker, its owner, as transaction `id` of
/// the caller `key`, with a new task and mailbox, taking no further request.
/// A pool that cannot start the task discards it.
fn reinstate<T, K>(shared: &Weak<Shared<T, K>>, id: TxnId, key: K, txn: T)
where
    T: MaybeSend + 'static,
    K: Clone + PartialEq + Send + Sync + 'static,
{
    let Some(tables) = shared.upgrade() else {
        return;
    };
    let worker = id.owner(tables.tables.len());
    let (mailbox, inbox) = unbounded();
    let idle = Arc::new(ArrayQueue::new(1));
    // The slot is new and holds one: the push cannot find it full.
    let _ = idle.push(txn);
    tables.tables[worker].insert(
        id,
        Entry {
            mailbox,
            idle: Arc::clone(&idle),
            key,
            ending: true,
        },
    );
    let status = tables.outcomes[worker]
        .cell(id)
        .unwrap_or_else(|| Arc::new(AtomicU8::new(Outcome::Open as u8)));
    if start(actor::own(idle, id, worker, inbox, shared.clone(), status)).is_err() {
        tables.tables[worker].take(id);
    }
}

/// The page's thread is the one worker, whether or not it is driving the
/// pool when a transaction begins.
#[cfg(target_family = "wasm")]
fn here() -> Result<usize, Unavailable> {
    Ok(0)
}

/// Starts a transaction's actor on the calling worker, as a daemon: pool
/// shutdown does not wait for it, and it is dropped (its transaction
/// discarded) when the worker stops.
#[cfg(not(target_family = "wasm"))]
fn start<F>(actor: F) -> Result<(), workers::Error>
where
    F: Future<Output = ()> + 'static,
{
    workers::spawn_daemon(actor)
}

/// On the page's worker the actor is one of its daemons; called while the
/// page itself runs (a test, a call that did not come through the pool) the
/// browser's executor runs it, on the same thread.
#[cfg(target_family = "wasm")]
fn start<F>(actor: F) -> Result<(), workers::Error>
where
    F: Future<Output = ()> + 'static,
{
    if workers::on_worker() {
        return workers::spawn_daemon(actor);
    }
    drop(n0_future::task::spawn(actor));
    Ok(())
}
