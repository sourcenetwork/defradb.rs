//! A worker's regolith queues, one per database, in its thread-local
//! registry.
//!
//! The registry exists only on a thread whose [`Bound`] lives (the pool
//! worker that built its hook). A queue is opened the first time the thread
//! asks for a database, and dropped, on this thread, once that database is
//! gone or the worker stops: dropping a queue completes every wait recorded
//! on it, so it must happen on its owner.
//!
//! A queue is polled without the registry borrowed: a completion a poll
//! delivers runs regolith callbacks, and one of them may open another
//! transaction here, which looks the registry up. Each entry is its own
//! `Rc`, cloned out for the poll.
//!
//! The registry keeps the waker its worker rested with until the worker runs
//! again, so a queue opened while it rests (a browser page's call on its
//! driven worker's thread) wakes the worker like the others do.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Weak};
use std::task::Waker;

use regolith::{IoBudget, IoQueue, OptimisticTransactionDb, QueueId};

struct Entry {
    /// Held weakly, so the registry never keeps a closed database alive;
    /// the allocation it points at outlives the entry, so the address
    /// compared on lookup can never belong to another database.
    db: Weak<OptimisticTransactionDb>,
    id: QueueId,
    queue: Rc<RefCell<IoQueue>>,
}

#[derive(Default)]
struct Registry {
    entries: Vec<Entry>,
    /// The waker the worker rested with; `None` while it runs.
    resting: Option<Waker>,
}

thread_local! {
    /// `None` on a thread that is not a worker with regolith queues.
    static QUEUES: RefCell<Option<Registry>> = const { RefCell::new(None) };
}

/// The calling thread's registry, for as long as it lives.
pub(super) struct Bound(());

impl Bound {
    /// Opens the calling thread's registry.
    pub(super) fn here() -> Bound {
        QUEUES.with(|queues| {
            queues.borrow_mut().get_or_insert_with(Registry::default);
        });
        Bound(())
    }
}

impl Drop for Bound {
    /// Closes the registry and drops its queues here, on their owner.
    fn drop(&mut self) {
        let entries = QUEUES.with(|queues| queues.borrow_mut().take());
        drop(entries);
    }
}

pub(super) fn queue_for(db: &Arc<OptimisticTransactionDb>) -> Option<QueueId> {
    QUEUES.with(|queues| {
        let mut queues = queues.borrow_mut();
        let registry = queues.as_mut()?;
        let target = Arc::as_ptr(db);
        if let Some(entry) = registry
            .entries
            .iter()
            .find(|entry| std::ptr::eq(entry.db.as_ptr(), target))
        {
            return Some(entry.id);
        }
        let queue = db.db().io_queue();
        let id = queue.id();
        if let Some(waker) = &registry.resting {
            queue.idle_waker(waker);
        }
        registry.entries.push(Entry {
            db: Arc::downgrade(db),
            id,
            queue: Rc::new(RefCell::new(queue)),
        });
        Some(id)
    })
}

/// The `index`th queue, or that it is gone: the entry of a database that was
/// dropped is taken out and handed back to be dropped by the caller, outside
/// the registry's borrow.
enum Slot {
    Live(Rc<RefCell<IoQueue>>),
    Gone(Entry),
    End,
}

fn slot(index: usize) -> Slot {
    QUEUES.with(|queues| {
        let mut queues = queues.borrow_mut();
        let Some(registry) = queues.as_mut() else {
            return Slot::End;
        };
        let entries = &mut registry.entries;
        match entries.get(index) {
            None => Slot::End,
            Some(entry) if entry.db.strong_count() == 0 => Slot::Gone(entries.swap_remove(index)),
            Some(entry) => Slot::Live(Rc::clone(&entry.queue)),
        }
    })
}

/// Polls every queue of the calling worker within `budget` units each.
/// Records whether the worker rests, and with which waker.
fn set_resting(waker: Option<&Waker>) {
    QUEUES.with(|queues| {
        if let Some(registry) = queues.borrow_mut().as_mut() {
            registry.resting = waker.cloned();
        }
    });
}

pub(super) fn poll(budget: usize) -> workers::HookProgress {
    set_resting(None);
    let mut progress = workers::HookProgress::default();
    let mut index = 0;
    loop {
        match slot(index) {
            Slot::End => return progress,
            Slot::Gone(entry) => drop(entry),
            Slot::Live(queue) => {
                index += 1;
                let Ok(mut queue) = queue.try_borrow_mut() else {
                    continue;
                };
                let polled = queue.poll(IoBudget::units(budget));
                if polled.retained_live_bytes > 0 {
                    tracing::trace!(
                        retained_live_bytes = polled.retained_live_bytes,
                        "regolith queue retains landed bytes"
                    );
                }
                progress.completed += polled.completed;
                progress.more |= polled.more_pending;
                progress.wake_in = match (progress.wake_in, polled.wake_in) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
            }
        }
    }
}

/// Hands `waker` to every queue of the calling worker before it parks, then
/// flushes the thread's kovan state.
///
/// kovan reclaims what a thread retired only once that thread's batch fills
/// or it flushes; a parked worker does neither, so map nodes its polls
/// retired (and the decoded blocks they hold) would stay alive for as long as
/// it rests. Flushing here hands them to reclamation before the park.
pub(super) fn idle(waker: &Waker) {
    set_resting(Some(waker));
    let mut index = 0;
    loop {
        match slot(index) {
            Slot::End => break,
            Slot::Gone(entry) => drop(entry),
            Slot::Live(queue) => {
                index += 1;
                queue.borrow().idle_waker(waker);
            }
        }
    }
    kovan::flush();
}

/// How many queues the calling thread holds.
#[cfg(test)]
pub(super) fn len() -> usize {
    QUEUES.with(|queues| {
        queues
            .borrow()
            .as_ref()
            .map_or(0, |registry| registry.entries.len())
    })
}
