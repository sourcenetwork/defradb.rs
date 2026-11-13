//! Callbacks a transaction runs when it ends, and the one registry that holds
//! them.
//!
//! A transaction ends once: it commits, or it ends without committing (its
//! commit fails, or it is discarded or dropped). What was registered with
//! [`Txn::on_commit`](super::Txn::on_commit) runs only after a commit, what
//! was registered with [`Txn::on_abort`](super::Txn::on_abort) only after any
//! other end; each callback runs once, and the other end's are dropped unrun.
//!
//! After a commit, in this order: the hooks ([`OnCommit::OrderedAsync`]) are
//! awaited and the events ([`OnCommit::Ordered`]) handed to the stream that
//! sends every commit's events out in commit-receipt order; then the async
//! callbacks are awaited and the sync ones run. [`OnCommit::OrderedAndWait`]
//! adds its publication completion to those awaited callbacks. After any other end the
//! async callbacks come first too: awaited after a failed commit, spawned
//! after a discard or a drop, whose caller cannot await. Each kind keeps its
//! registration order.
//!
//! [`Txn::before_commit`](super::Txn::before_commit) registers work that runs
//! just before the commit is attempted, in registration order; an error from
//! one fails the commit and its abort callbacks run. A transaction that
//! registers none pays nothing for it: no allocation and no pass.
//!
//! Every callback type is one shape of [`CallOnce`], declared once for every
//! target: `Send` on native targets, not required to be in the
//! single-threaded browser ([`MaybeSend`]).

use std::future::Future;
use std::sync::Arc;

use defra_core::thread_bounds::MaybeBoxFuture;
use regolith::sync::{Event, OnceCell, SegQueue};

use super::errors::{Error, Result};
use super::traits::MaybeSend;

/// A callback that runs once and returns `T`.
pub trait CallOnce<T>: FnOnce() -> T + MaybeSend + 'static {}

impl<F, T> CallOnce<T> for F where F: FnOnce() -> T + MaybeSend + 'static {}

/// A callback run once, synchronously.
pub type TxnCallback = Box<dyn CallOnce<()>>;

/// A callback awaited once.
pub type AsyncTxnCallback = Box<dyn CallOnce<MaybeBoxFuture<'static, ()>>>;

/// A hook awaited once its commit is durable, before that commit's events go
/// out. Its error is reported, never returned: the commit stands.
pub type CommitHook = Box<dyn CallOnce<MaybeBoxFuture<'static, std::result::Result<(), String>>>>;

/// Work run once just before the commit is attempted; an error fails the
/// commit.
pub type BeforeCommit = Box<dyn CallOnce<Result<()>>>;

/// What runs after a commit, and how.
pub enum OnCommit {
    /// Run once the commit is durable.
    Sync(TxnCallback),
    /// Awaited once the commit is durable.
    Async(AsyncTxnCallback),
    /// An event: sent out with every commit's events, in commit-receipt
    /// order.
    Ordered(TxnCallback),
    /// An ordered publication that must finish before commit returns, such as
    /// a schema cache needed by the caller's next request. Other commits do
    /// not wait for this callback unless they request the same guarantee.
    OrderedAndWait(TxnCallback),
    /// A hook: awaited before this commit's events go out, which holds back
    /// the events of every later commit until it finishes.
    OrderedAsync(CommitHook),
}

impl OnCommit {
    /// Whether it is ordered by the commit's receipt.
    fn is_ordered(&self) -> bool {
        matches!(
            self,
            Self::Ordered(_) | Self::OrderedAndWait(_) | Self::OrderedAsync(_)
        )
    }

    /// Whether a transaction that is `read_only` takes it: one never gets a
    /// commit receipt to order work by, so it refuses ordered work.
    pub fn admitted_by(&self, read_only: bool) -> Result<()> {
        if read_only && self.is_ordered() {
            return Err(Error::ReadOnlyTxn);
        }
        Ok(())
    }
}

/// What runs after a transaction ends without committing.
pub enum OnAbort {
    /// Run once the transaction has ended.
    Sync(TxnCallback),
    /// Awaited once a failed commit returns, or spawned after a discard or a
    /// drop.
    Async(AsyncTxnCallback),
}

/// The callbacks one transaction registered, in registration order.
///
/// The queue is what makes the registry `Sync` without a lock (an attempt
/// view holding it is shared by reference across awaits), and it is built on
/// the first registration, so a transaction that registers nothing allocates
/// nothing.
#[derive(Default)]
pub struct TxnCallbacks {
    read_only: bool,
    entries: Option<Box<SegQueue<Entry>>>,
    /// Whether any before-commit work was registered, so a commit with none
    /// skips that pass entirely.
    before: bool,
}

enum Entry {
    Before(BeforeCommit),
    Commit(OnCommit),
    Abort(OnAbort),
}

/// How many callbacks a registry held when a savepoint was set.
#[derive(Clone, Copy, Debug, Default)]
pub struct CallbackMark(usize);

impl TxnCallbacks {
    /// A registry for a transaction that is `read_only` or not. A read-only
    /// one refuses ordered work: it never gets a commit receipt to order it
    /// by.
    pub fn new(read_only: bool) -> Self {
        Self {
            read_only,
            entries: None,
            before: false,
        }
    }

    /// Register `callback` to run once just before the commit is attempted.
    pub fn before_commit(&mut self, callback: BeforeCommit) {
        self.before = true;
        self.push(Entry::Before(callback));
    }

    /// Run the before-commit work, in registration order, and keep every
    /// other callback in its place. The first error stops the pass and is
    /// returned; the commit must then fail. With nothing registered this
    /// returns at once.
    pub fn run_before_commit(&mut self) -> Result<()> {
        if !std::mem::take(&mut self.before) {
            return Ok(());
        }
        let Some(entries) = &self.entries else {
            return Ok(());
        };
        let mut rest = Vec::with_capacity(entries.len());
        let mut before = Vec::new();
        while let Some(entry) = entries.pop() {
            match entry {
                Entry::Before(callback) => before.push(callback),
                other => rest.push(other),
            }
        }
        for entry in rest {
            entries.push(entry);
        }
        before.into_iter().try_for_each(|callback| callback())
    }

    /// Register `callback` to run once if the transaction commits.
    pub fn on_commit(&mut self, callback: OnCommit) -> Result<()> {
        callback.admitted_by(self.read_only)?;
        self.push(Entry::Commit(callback));
        Ok(())
    }

    /// Register `callback` to run once if the transaction ends without
    /// committing.
    pub fn on_abort(&mut self, callback: OnAbort) {
        self.push(Entry::Abort(callback));
    }

    fn push(&mut self, entry: Entry) {
        self.entries.get_or_insert_with(Box::default).push(entry);
    }

    /// The callbacks registered so far, of every kind.
    pub fn len(&self) -> usize {
        self.entries.as_ref().map_or(0, |entries| entries.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Where the registry stands, for a savepoint to return to.
    pub fn mark(&self) -> CallbackMark {
        CallbackMark(self.len())
    }

    /// Drop the callbacks registered after `mark`.
    pub fn truncate(&mut self, mark: CallbackMark) {
        let Some(entries) = &self.entries else {
            return;
        };
        let mut kept = Vec::with_capacity(mark.0.min(entries.len()));
        while kept.len() < mark.0 {
            match entries.pop() {
                Some(entry) => kept.push(entry),
                None => break,
            }
        }
        while entries.pop().is_some() {}
        for entry in kept {
            entries.push(entry);
        }
    }

    /// Register what `other` holds after what this registry holds.
    pub fn append(&mut self, other: Self) {
        self.before |= other.before;
        let Some(other) = other.entries else {
            return;
        };
        while let Some(entry) = other.pop() {
            self.push(entry);
        }
    }

    /// What runs after a commit, and what runs after any other end.
    pub fn into_ends(self) -> (Committed, Ended) {
        let mut committed = Committed::default();
        let mut aborted = Ended::default();
        let Some(entries) = self.entries else {
            return (committed, aborted);
        };
        while let Some(entry) = entries.pop() {
            match entry {
                // Run by `run_before_commit` before any commit; one that is
                // still here belongs to a transaction that never committed.
                Entry::Before(_) => {}
                Entry::Commit(OnCommit::Sync(callback)) => committed.then.sync.push(callback),
                Entry::Commit(OnCommit::Async(callback)) => {
                    committed.then.asynchronous.push(callback)
                }
                Entry::Commit(OnCommit::Ordered(event)) => committed.events.push(event),
                Entry::Commit(OnCommit::OrderedAndWait(event)) => {
                    let signal = Arc::new(Event::new());
                    let done = PublicationDone(Arc::clone(&signal));
                    committed.events.push(Box::new(move || {
                        let _done = done;
                        event();
                    }));
                    committed.then.asynchronous.push(Box::new(move || {
                        Box::pin(async move { signal.wait().await })
                    }));
                }
                Entry::Commit(OnCommit::OrderedAsync(hook)) => committed.hooks.push(hook),
                Entry::Abort(OnAbort::Sync(callback)) => aborted.sync.push(callback),
                Entry::Abort(OnAbort::Async(callback)) => aborted.asynchronous.push(callback),
            }
        }
        (committed, aborted)
    }
}

/// A callback registered through a shared handle of a transaction, waiting
/// for the transaction's own registry.
pub enum Registration {
    BeforeCommit(BeforeCommit),
    Commit(OnCommit),
    Abort(OnAbort),
}

/// Where queued registrations land: a transaction's registry, or the
/// transaction itself.
pub trait CallbackSink {
    fn register(&mut self, registration: Registration) -> Result<()>;
}

/// A sink is anything with the three registrations a transaction has.
macro_rules! callback_sink {
    ($($sink:ty),+) => {$(
        impl CallbackSink for $sink {
            fn register(&mut self, registration: Registration) -> Result<()> {
                match registration {
                    Registration::BeforeCommit(callback) => self.before_commit(callback),
                    Registration::Commit(callback) => self.on_commit(callback)?,
                    Registration::Abort(callback) => self.on_abort(callback),
                }
                Ok(())
            }
        }
    )+};
}

callback_sink!(TxnCallbacks, Box<dyn super::Txn>);

/// Callbacks registered through `&self`, by the handles that share one
/// transaction (its namespace views, the request components it is lent
/// to), queued in registration order until the transaction's own registry
/// takes them. Registering never waits and takes no lock; the transaction
/// settles the queue before anything that depends on what is registered (a
/// savepoint, a registration of its own, its commit or its discard), so the
/// registration order holds across every handle.
#[derive(Default)]
pub struct QueuedCallbacks {
    read_only: bool,
    queue: OnceCell<Box<SegQueue<Registration>>>,
}

impl QueuedCallbacks {
    /// A queue for a transaction that is `read_only` or not; a read-only one
    /// refuses ordered work at once, as its registry would.
    pub fn new(read_only: bool) -> Self {
        Self {
            read_only,
            queue: OnceCell::new(),
        }
    }

    /// Register `callback` to run once if the transaction commits.
    pub fn on_commit(&self, callback: OnCommit) -> Result<()> {
        callback.admitted_by(self.read_only)?;
        self.push(Registration::Commit(callback));
        Ok(())
    }

    /// Register `callback` to run once if the transaction ends without
    /// committing.
    pub fn on_abort(&self, callback: OnAbort) {
        self.push(Registration::Abort(callback));
    }

    /// Register `callback` to run once just before the commit is attempted.
    pub fn before_commit(&self, callback: BeforeCommit) {
        self.push(Registration::BeforeCommit(callback));
    }

    fn push(&self, registration: Registration) {
        self.queue.get_or_init_racy(Box::default).push(registration);
    }

    /// Hands every queued registration to `sink`, in order.
    pub fn settle_into(&self, sink: &mut impl CallbackSink) -> Result<()> {
        let Some(queue) = self.queue.get() else {
            return Ok(());
        };
        while let Some(registration) = queue.pop() {
            sink.register(registration)?;
        }
        Ok(())
    }

    /// The registrations waiting.
    pub fn len(&self) -> usize {
        self.queue.get().map_or(0, |queue| queue.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What runs after a commit, each kind in registration order.
#[derive(Default)]
pub struct Committed {
    /// Sent out in commit-receipt order.
    pub events: Vec<TxnCallback>,
    /// Awaited before the events go out.
    pub hooks: Vec<CommitHook>,
    /// Run once the hooks finished.
    pub then: Ended,
}

/// The callbacks of one end that nothing orders: the async ones run before
/// the sync ones, each kind in registration order.
#[derive(Default)]
pub struct Ended {
    asynchronous: Vec<AsyncTxnCallback>,
    sync: Vec<TxnCallback>,
}

impl Ended {
    /// Await the async callbacks, then run the sync ones. A panic in one
    /// unwinds to the caller, and the callbacks after it do not run.
    pub async fn run(self) {
        for callback in self.asynchronous {
            callback().await;
        }
        for callback in self.sync {
            callback();
        }
    }

    /// For an end whose caller cannot await, a discard or a drop: the async
    /// callbacks are spawned, one task running them in order, then the sync
    /// ones run.
    pub fn run_now(self) {
        if !self.asynchronous.is_empty() {
            spawn_in_order(self.asynchronous);
        }
        for callback in self.sync {
            callback();
        }
    }
}

/// Runs `callbacks` in order as one detached task: on the worker that ended
/// the transaction, or, off the pool, on the process's default pool.
fn spawn_in_order(callbacks: Vec<AsyncTxnCallback>) {
    let count = callbacks.len();
    let run = async move {
        for callback in callbacks {
            callback().await;
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    let spawned = workers::spawn_detached(None, "storage::abort_callbacks", move || run);
    // The browser's one worker is the page's thread, where every transaction
    // ends, and its callbacks are not `Send`.
    #[cfg(target_arch = "wasm32")]
    let spawned = workers::spawn_local(run).map(drop);
    if let Err(error) = spawned {
        tracing::error!(
            count,
            %error,
            "async abort callbacks did not run: no worker pool could take them"
        );
    }
}

/// Helper function to create a simple sync callback from a closure.
///
/// # Example
///
/// ```ignore
/// txn.on_commit(OnCommit::Sync(make_callback(|| println!("Success!"))))?;
/// ```
pub fn make_callback<F>(f: F) -> TxnCallback
where
    F: FnOnce() + MaybeSend + 'static,
{
    Box::new(f)
}

/// Helper function to create an async callback from a closure.
///
/// # Example
///
/// ```ignore
/// txn.on_commit(OnCommit::Async(make_async_callback(|| async {
///     println!("Async success!");
/// })))?;
/// ```
pub fn make_async_callback<F, Fut>(f: F) -> AsyncTxnCallback
where
    F: FnOnce() -> Fut + MaybeSend + 'static,
    Fut: Future<Output = ()> + MaybeSend + 'static,
{
    Box::new(move || Box::pin(f()))
}

#[cfg(test)]
mod tests;

/// An event panic must release its caller too; the dispatcher reports it.
struct PublicationDone(Arc<Event>);

impl Drop for PublicationDone {
    fn drop(&mut self) {
        self.0.set();
    }
}
