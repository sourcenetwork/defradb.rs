//! Receipt-ordered dispatch of post-commit work.
//!
//! The events of a commit (bus messages, in-memory updates, hand-offs to the
//! P2P layer) go out in the order of the commits' receipts
//! (`CommitReceipt::seq`), whichever thread finishes first.
//!
//! A commit that carries ordered work is bound ([`EventOrder::bind`]) just
//! before it enters regolith: its owner queues a `Begin`, and regolith's own
//! `on_commit` and `on_abort` callbacks queue its one outcome, so every
//! attempt ends exactly once whatever ends it (a commit, a conflict, an
//! error, a drop, a close). The owner hands the events over once it knows
//! the outcome ([`Binding::hand_over`]), runs the commit's hooks, and says
//! when they finished ([`HooksRunning::done`]); a commit with hooks holds the
//! stream back until then. Dropping either handle unfinished hands over
//! what it holds, so an owner that is cancelled never stalls the stream.
//!
//! One thread at a time drains: whoever takes the one `Drainer` out of its
//! queue, and nobody waits for it. It applies the queued messages in order
//! and sends out every event that no commit still in flight could precede. A
//! commit's floor is the highest position read when its `Begin` was read;
//! its receipt cannot be lower, because the receipt is assigned after the
//! `Begin` was queued and every outcome read before that was assigned
//! earlier. A commit that writes nothing has its snapshot as its receipt; it
//! goes out at that receipt or, when later receipts already went out, right
//! after them, at its floor.
//!
//! Queueing a message is O(1). Applying one is O(1), and sending an event
//! out is O(log n), n being the events held behind a commit still in flight.
//! Nothing copies state. A panicking event is caught and reported
//! (`post_commit::report`), and the stream goes on.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{self, AtomicU64};
use std::sync::Arc;

use rapidhash::fast::RapidHashMap;
use regolith::sync::SegQueue;
use regolith::Transaction;
use telemetry::StorageEvent;

/// An event, dispatched once, in receipt order.
pub(super) use crate::corekv::TxnCallback as Event;

/// A commit's life as regolith reports it. Numbers only, so the regolith
/// callbacks that queue it are `Send` on every target.
enum Life {
    Begin(u64),
    Committed(u64, u64),
    Aborted(u64),
}

/// What an owner hands over.
enum Work {
    Attach {
        token: u64,
        events: Vec<Event>,
        hooks: bool,
    },
    HooksDone(u64),
}

/// Shared by every transaction of one store.
pub(crate) struct EventOrder {
    next_token: AtomicU64,
    life: Arc<SegQueue<Life>>,
    work: SegQueue<Work>,
    /// Holds the one `Drainer` while nobody drains.
    drainer: SegQueue<Box<Drainer>>,
}

impl Default for EventOrder {
    fn default() -> Self {
        let drainer = SegQueue::new();
        drainer.push(Box::default());
        Self {
            next_token: AtomicU64::new(0),
            life: Arc::new(SegQueue::new()),
            work: SegQueue::new(),
            drainer,
        }
    }
}

impl EventOrder {
    /// Binds a commit that carries `events` (and, with `hooks`, hooks its
    /// owner runs after it) to the stream, just before `txn` enters regolith.
    pub(super) fn bind(
        self: &Arc<Self>,
        txn: &mut Transaction,
        events: Vec<Event>,
        hooks: bool,
    ) -> Binding {
        let token = self.next_token.fetch_add(1, atomic::Ordering::Relaxed);
        self.life.push(Life::Begin(token));
        let life = Arc::clone(&self.life);
        txn.on_commit(move |receipt| life.push(Life::Committed(token, receipt.seq())));
        let life = Arc::clone(&self.life);
        txn.on_abort(move |_| life.push(Life::Aborted(token)));
        Binding {
            order: Arc::clone(self),
            token,
            events: Some(events),
            hooks,
        }
    }

    fn hand_over(&self, token: u64, events: Vec<Event>, hooks: bool) {
        self.work.push(Work::Attach {
            token,
            events,
            hooks,
        });
        self.drain();
    }

    fn hooks_done(&self, token: u64) {
        self.work.push(Work::HooksDone(token));
        self.drain();
    }

    /// Sends out whatever the floors allow, unless another thread is
    /// draining: then that thread finds these messages before it stops.
    /// A thread that is unwinding only queues: an event panicking under it
    /// would abort the process, and the next drain sends what it queued.
    fn drain(&self) {
        if std::thread::panicking() {
            return;
        }
        while let Some(mut drainer) = self.drainer.pop() {
            drainer.run(&self.life, &self.work);
            self.drainer.push(drainer);
            // A message queued while this thread drained found no drainer
            // to take; it was queued before the drainer went back, so it
            // shows here.
            if self.life.is_empty() && self.work.is_empty() {
                return;
            }
        }
    }
}

/// A bound commit's ordered work, handed over exactly once.
pub(super) struct Binding {
    order: Arc<EventOrder>,
    token: u64,
    events: Option<Vec<Event>>,
    hooks: bool,
}

impl Binding {
    /// The owner knows whether regolith committed. A commit that did hands
    /// its events over; with hooks they wait for [`HooksRunning::done`].
    pub(super) fn hand_over(mut self, committed: bool) -> Option<HooksRunning> {
        let events = match (committed, self.events.take()) {
            (true, Some(events)) => events,
            _ => Vec::new(),
        };
        let hooks = committed && self.hooks;
        self.order.hand_over(self.token, events, hooks);
        hooks.then(|| HooksRunning {
            order: Arc::clone(&self.order),
            token: self.token,
            done: false,
        })
    }
}

impl Drop for Binding {
    /// An owner cancelled before it learned the outcome: the events go out
    /// if regolith committed, and the hooks are reported as not run.
    fn drop(&mut self) {
        let Some(events) = self.events.take() else {
            return;
        };
        if self.hooks {
            super::post_commit::report(
                StorageEvent::PostCommitHookCancelled,
                &"the commit's owner was cancelled before its hooks ran",
            );
        }
        self.order.hand_over(self.token, events, false);
    }
}

/// The hooks of a committed commit, running; the stream waits for them.
pub(super) struct HooksRunning {
    order: Arc<EventOrder>,
    token: u64,
    done: bool,
}

impl HooksRunning {
    pub(super) fn done(mut self) {
        self.done = true;
        self.order.hooks_done(self.token);
    }
}

impl Drop for HooksRunning {
    /// An owner cancelled while its hooks ran: the events still go out.
    fn drop(&mut self) {
        if !self.done {
            super::post_commit::report(
                StorageEvent::PostCommitHookCancelled,
                &"the commit's owner was cancelled while its hooks ran",
            );
            self.order.hooks_done(self.token);
        }
    }
}

/// What the drainer knows of one bound commit.
#[derive(Default)]
struct Slot {
    /// Set when its `Begin` is read.
    floor: Option<u64>,
    /// Set when its commit is read: its receipt, raised to its floor.
    position: Option<u64>,
    events: Option<Vec<Event>>,
    attached: bool,
    hooks: bool,
    /// It holds nothing back any more: it failed, or its events are ready.
    settled: bool,
}

/// One commit's events, waiting to go out.
struct Ready {
    position: u64,
    arrival: u64,
    events: Vec<Event>,
}

impl PartialEq for Ready {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Ready {}

impl PartialOrd for Ready {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ready {
    /// Reversed, so the heap's top is the lowest position, then the oldest.
    fn cmp(&self, other: &Self) -> Ordering {
        (other.position, other.arrival).cmp(&(self.position, self.arrival))
    }
}

/// The state only the thread that drains touches.
#[derive(Default)]
struct Drainer {
    slots: RapidHashMap<u64, Slot>,
    /// Bound commits in the order their `Begin` was read. Floors never
    /// decrease along it, so the first unsettled one has the lowest floor.
    holding: VecDeque<u64>,
    ready: BinaryHeap<Ready>,
    /// The highest position read so far; a new floor starts here.
    completed: u64,
    arrivals: u64,
}

impl Drainer {
    fn run(&mut self, life: &SegQueue<Life>, work: &SegQueue<Work>) {
        while let Some(message) = life.pop() {
            self.apply_life(message);
        }
        while let Some(message) = work.pop() {
            self.apply_work(message);
        }
        self.dispatch();
    }

    fn apply_life(&mut self, message: Life) {
        match message {
            Life::Begin(token) => {
                self.slots.entry(token).or_default().floor = Some(self.completed);
                self.holding.push_back(token);
            }
            Life::Committed(token, receipt) => {
                let slot = self.slots.entry(token).or_default();
                let position = receipt.max(slot.floor.unwrap_or(receipt));
                slot.position = Some(position);
                self.completed = self.completed.max(position);
                self.complete(token);
            }
            Life::Aborted(token) => {
                let slot = self.slots.entry(token).or_default();
                slot.settled = true;
                slot.events = None;
                self.forget(token);
            }
        }
    }

    fn apply_work(&mut self, message: Work) {
        match message {
            Work::Attach {
                token,
                events,
                hooks,
            } => {
                let slot = self.slots.entry(token).or_default();
                slot.attached = true;
                slot.hooks = hooks;
                if !slot.settled {
                    slot.events = Some(events);
                }
                self.complete(token);
                self.forget(token);
            }
            Work::HooksDone(token) => {
                if let Some(slot) = self.slots.get_mut(&token) {
                    slot.hooks = false;
                }
                self.complete(token);
                self.forget(token);
            }
        }
    }

    /// A commit whose position and events are both in, with no hook
    /// running, joins the heap and stops holding the stream back.
    fn complete(&mut self, token: u64) {
        let Some(slot) = self.slots.get_mut(&token) else {
            return;
        };
        let Some(position) = slot.position else {
            return;
        };
        if slot.settled || !slot.attached || slot.hooks {
            return;
        }
        slot.settled = true;
        let events = slot.events.take().unwrap_or_default();
        if !events.is_empty() {
            self.ready.push(Ready {
                position,
                arrival: self.arrivals,
                events,
            });
            self.arrivals += 1;
        }
    }

    /// A commit that is settled and handed over has nothing more to say.
    fn forget(&mut self, token: u64) {
        if self
            .slots
            .get(&token)
            .is_some_and(|slot| slot.settled && slot.attached)
        {
            self.slots.remove(&token);
        }
    }

    /// The lowest floor of a commit still in flight, if any.
    fn floor(&mut self) -> Option<u64> {
        while let Some(token) = self.holding.front() {
            match self.slots.get(token) {
                Some(slot) if !slot.settled => return slot.floor,
                _ => {
                    self.holding.pop_front();
                }
            }
        }
        None
    }

    fn dispatch(&mut self) {
        let floor = self.floor();
        while self
            .ready
            .peek()
            .is_some_and(|next| floor.is_none_or(|floor| next.position <= floor))
        {
            let Some(next) = self.ready.pop() else {
                return;
            };
            for event in next.events {
                if let Err(panic) = catch_unwind(AssertUnwindSafe(event)) {
                    super::post_commit::report(
                        StorageEvent::PostCommitEventPanicked,
                        &panic_message(panic.as_ref()),
                    );
                }
            }
        }
    }
}

/// What a caught panic said, when it said it as text.
pub(super) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use regolith::sync::Atom;
    use regolith::{OptimisticTransactionDb, Options, TxnOptions};

    struct Fixture {
        db: OptimisticTransactionDb,
        order: Arc<EventOrder>,
        out: Arc<Atom<Vec<u64>>>,
    }

    fn record(out: &Atom<Vec<u64>>, value: u64) {
        out.rcu(|values| {
            let mut next = values.clone();
            next.push(value);
            next
        });
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                db: OptimisticTransactionDb::open("event-order", Options::memory()).unwrap(),
                order: Arc::new(EventOrder::default()),
                out: Arc::new(Atom::new(Vec::new())),
            }
        }

        fn event(&self, value: u64) -> Event {
            let out = Arc::clone(&self.out);
            Box::new(move || record(&out, value))
        }

        /// A writing transaction bound with one event, not yet committed.
        fn bound(&self, key: &[u8], value: u64, hooks: bool) -> (Transaction, Binding) {
            let mut txn = self.db.begin(&TxnOptions::new());
            txn.put(key, b"v").unwrap();
            let binding = self.order.bind(&mut txn, vec![self.event(value)], hooks);
            (txn, binding)
        }

        fn out(&self) -> Vec<u64> {
            self.out.load().clone()
        }
    }

    #[test]
    fn a_later_receipt_waits_for_an_earlier_commit_still_in_flight() {
        let fixture = Fixture::new();
        let (first, first_binding) = fixture.bound(b"a", 1, false);
        let (second, second_binding) = fixture.bound(b"b", 2, false);
        first.commit().unwrap();
        second.commit().unwrap();
        assert!(second_binding.hand_over(true).is_none());
        assert!(fixture.out().is_empty());
        first_binding.hand_over(true);
        assert_eq!(fixture.out(), vec![1, 2]);
    }

    #[test]
    fn events_from_one_commit_keep_registration_order() {
        let fixture = Fixture::new();
        let mut txn = fixture.db.begin(&TxnOptions::new());
        txn.put(b"a", b"v").unwrap();
        let binding =
            fixture
                .order
                .bind(&mut txn, vec![fixture.event(10), fixture.event(11)], false);
        txn.commit().unwrap();
        binding.hand_over(true);
        assert_eq!(fixture.out(), vec![10, 11]);
    }

    #[test]
    fn an_aborted_commit_releases_the_stream_and_sends_nothing() {
        let fixture = Fixture::new();
        let (failed, failed_binding) = fixture.bound(b"a", 1, false);
        let (later, later_binding) = fixture.bound(b"b", 2, false);
        later.commit().unwrap();
        later_binding.hand_over(true);
        assert!(fixture.out().is_empty());
        failed.rollback();
        failed_binding.hand_over(false);
        assert_eq!(fixture.out(), vec![2]);
        let drainer = fixture.order.drainer.pop().unwrap();
        assert!(drainer.slots.is_empty() && drainer.ready.is_empty());
    }

    #[test]
    fn a_dropped_transaction_is_an_abort_through_regolith() {
        let fixture = Fixture::new();
        let (dropped, dropped_binding) = fixture.bound(b"a", 1, false);
        let (later, later_binding) = fixture.bound(b"b", 2, false);
        later.commit().unwrap();
        later_binding.hand_over(true);
        drop(dropped);
        drop(dropped_binding);
        assert_eq!(fixture.out(), vec![2]);
    }

    #[test]
    fn hooks_hold_their_commit_and_everything_after_it() {
        let fixture = Fixture::new();
        let (hooked, hooked_binding) = fixture.bound(b"a", 1, true);
        let (later, later_binding) = fixture.bound(b"b", 2, false);
        hooked.commit().unwrap();
        later.commit().unwrap();
        later_binding.hand_over(true);
        let running = hooked_binding.hand_over(true).unwrap();
        assert!(fixture.out().is_empty());
        running.done();
        assert_eq!(fixture.out(), vec![1, 2]);
    }

    #[test]
    fn hooks_cancelled_with_their_owner_still_let_the_events_out() {
        let fixture = Fixture::new();
        let before = telemetry::storage_event_count(StorageEvent::PostCommitHookCancelled);
        let (hooked, hooked_binding) = fixture.bound(b"a", 1, true);
        hooked.commit().unwrap();
        drop(hooked_binding.hand_over(true));
        assert_eq!(fixture.out(), vec![1]);
        assert!(telemetry::storage_event_count(StorageEvent::PostCommitHookCancelled) > before);
    }

    #[test]
    fn an_owner_cancelled_before_the_outcome_still_sends_a_committed_commit() {
        let fixture = Fixture::new();
        let (txn, binding) = fixture.bound(b"a", 1, false);
        txn.commit().unwrap();
        drop(binding);
        assert_eq!(fixture.out(), vec![1]);
    }

    #[test]
    fn a_write_free_commit_goes_out_no_earlier_than_what_already_went_out() {
        let fixture = Fixture::new();
        let mut reader = fixture.db.begin(&TxnOptions::new());
        reader.get(b"a").unwrap();
        let (writer, writer_binding) = fixture.bound(b"a", 1, false);
        writer.commit().unwrap();
        writer_binding.hand_over(true);
        let reader_binding = fixture
            .order
            .bind(&mut reader, vec![fixture.event(0)], false);
        reader.commit().unwrap();
        reader_binding.hand_over(true);
        assert_eq!(fixture.out(), vec![1, 0]);
    }

    #[test]
    fn a_panicking_event_is_reported_and_the_stream_goes_on() {
        let fixture = Fixture::new();
        let before = telemetry::storage_event_count(StorageEvent::PostCommitEventPanicked);
        let mut txn = fixture.db.begin(&TxnOptions::new());
        txn.put(b"a", b"v").unwrap();
        let panicking: Event = Box::new(|| panic!("subscriber panic"));
        let binding = fixture
            .order
            .bind(&mut txn, vec![panicking, fixture.event(2)], false);
        txn.commit().unwrap();
        binding.hand_over(true);
        let (later, later_binding) = fixture.bound(b"b", 3, false);
        later.commit().unwrap();
        later_binding.hand_over(true);
        assert_eq!(fixture.out(), vec![2, 3]);
        assert!(telemetry::storage_event_count(StorageEvent::PostCommitEventPanicked) > before);
    }

    #[test]
    fn an_owner_that_panics_queues_its_events_for_the_next_drain() {
        let fixture = Fixture::new();
        let unwound = catch_unwind(AssertUnwindSafe(|| {
            let (txn, _binding) = fixture.bound(b"a", 1, false);
            txn.commit().unwrap();
            panic!("the owner panicked after its commit");
        }));
        assert!(unwound.is_err());
        assert!(fixture.out().is_empty());
        let (later, later_binding) = fixture.bound(b"b", 2, false);
        later.commit().unwrap();
        later_binding.hand_over(true);
        assert_eq!(fixture.out(), vec![1, 2]);
    }

    #[test]
    fn events_queued_by_an_event_go_out_after_it() {
        let fixture = Arc::new(Fixture::new());
        let inner = Arc::clone(&fixture);
        let mut txn = fixture.db.begin(&TxnOptions::new());
        txn.put(b"a", b"v").unwrap();
        let reentrant: Event = Box::new(move || {
            let (txn, binding) = inner.bound(b"b", 2, false);
            txn.commit().unwrap();
            binding.hand_over(true);
            record(&inner.out, 1);
        });
        let binding = fixture.order.bind(&mut txn, vec![reentrant], false);
        txn.commit().unwrap();
        binding.hand_over(true);
        assert_eq!(fixture.out(), vec![1, 2]);
    }
}
