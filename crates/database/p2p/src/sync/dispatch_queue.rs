//! Transport events queued by dispatch class, so a full class waits alone.
//!
//! One bounded lane per [`DispatchClass`]. Events leave in the order they were
//! sent, as from a single queue: an event whose send returned before another's
//! began leaves first. Sends that overlap have no order between them. When the completion workers are busy, the
//! dispatcher stops taking from the completion lane only; requests, recovery
//! and inline events keep flowing past it.

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use defra_core::thread_bounds::MaybeSend;
use regolith::sync::{unbounded, Notify, Semaphore, UnboundedReceiver, UnboundedSender};

use super::{DispatchClass, DispatchEvent};

/// Something an event holds until its handler returns, such as the bytes its
/// body was admitted against.
pub type Lease = Box<dyn MaybeSend>;

/// An event and the lease it carries, as the dispatcher receives it.
pub struct Queued<E> {
    pub event: E,
    pub lease: Option<Lease>,
}

const LANES: usize = 4;
const COMPLETION_LANE: usize = lane(DispatchClass::Completion);

const fn lane(class: DispatchClass) -> usize {
    match class {
        DispatchClass::Inline => 0,
        DispatchClass::Admission => 1,
        DispatchClass::Recovery => 2,
        DispatchClass::Completion => 3,
    }
}

/// A queue holding at most `capacity` events per dispatch class.
pub fn dispatch_queue<E>(capacity: usize) -> (DispatchSender<E>, DispatchReceiver<E>)
where
    E: DispatchEvent + 'static,
{
    let room: [Arc<Semaphore>; LANES] = std::array::from_fn(|_| Arc::new(Semaphore::new(capacity)));
    let signal = Arc::new(Signal::default());
    let pairs: [_; LANES] = std::array::from_fn(|_| unbounded::<Sequenced<E>>());
    let sender = DispatchSender {
        shared: Arc::new(SenderShared {
            lanes: pairs.each_ref().map(|(sender, _)| sender.clone()),
            room: room.clone(),
            signal: Arc::clone(&signal),
            next_seq: AtomicU64::new(0),
        }),
    };
    let receiver = DispatchReceiver {
        lanes: pairs.map(|(_, receiver)| receiver),
        heads: Default::default(),
        room,
        capacity,
        signal,
    };
    (sender, receiver)
}

/// A queued event and its place in send order.
type Sequenced<E> = (u64, Queued<E>);

/// What the two halves tell each other. One wake-up for every lane, so a
/// waiting receiver registers once rather than on each lane.
#[derive(Default)]
struct Signal {
    queued: Notify,
    senders_gone: AtomicBool,
    receiver_gone: AtomicBool,
}

struct SenderShared<E>
where
    E: 'static,
{
    lanes: [UnboundedSender<Sequenced<E>>; LANES],
    room: [Arc<Semaphore>; LANES],
    signal: Arc<Signal>,
    next_seq: AtomicU64,
}

impl<E> Drop for SenderShared<E>
where
    E: 'static,
{
    fn drop(&mut self) {
        self.signal.senders_gone.store(true, Ordering::Release);
        self.signal.queued.notify_one();
    }
}

/// The sending half of a [`dispatch_queue`]. Every clone feeds the same lanes.
pub struct DispatchSender<E>
where
    E: 'static,
{
    shared: Arc<SenderShared<E>>,
}

impl<E> Clone for DispatchSender<E>
where
    E: 'static,
{
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<E> DispatchSender<E>
where
    E: DispatchEvent + 'static,
{
    /// Waits for room in the event's lane, then queues it. Hands the event
    /// back once the receiver is gone.
    pub async fn send(&self, event: E) -> Result<(), E> {
        self.enqueue(event, None).await
    }

    /// Like [`send`](Self::send), with a lease the event holds until handled.
    pub async fn send_leased(&self, event: E, lease: Lease) -> Result<(), E> {
        self.enqueue(event, Some(lease)).await
    }

    async fn enqueue(&self, event: E, lease: Option<Lease>) -> Result<(), E> {
        let lane = lane(event.dispatch_class());
        let signal = &self.shared.signal;
        if signal.receiver_gone.load(Ordering::Acquire) {
            return Err(event);
        }
        self.shared.room[lane].acquire(1).await.forget();
        // The receiver wakes every waiting sender when it goes away.
        if signal.receiver_gone.load(Ordering::Acquire) {
            return Err(event);
        }
        let seq = self.shared.next_seq.fetch_add(1, Ordering::Relaxed);
        self.shared.lanes[lane]
            .try_send((seq, Queued { event, lease }))
            .map_err(|(_, queued)| queued.event)?;
        signal.queued.notify_one();
        Ok(())
    }
}

/// The receiving half of a [`dispatch_queue`].
pub struct DispatchReceiver<E>
where
    E: 'static,
{
    lanes: [UnboundedReceiver<Sequenced<E>>; LANES],
    /// The oldest event of each lane, taken off it to compare send order.
    heads: [Option<Sequenced<E>>; LANES],
    room: [Arc<Semaphore>; LANES],
    capacity: usize,
    signal: Arc<Signal>,
}

impl<E> DispatchReceiver<E>
where
    E: 'static,
{
    /// The oldest event in any lane. `None` once every sender is gone and
    /// every lane is empty.
    pub async fn recv(&mut self) -> Option<E> {
        self.next_queued(true).await.map(|queued| queued.event)
    }

    /// Whether every sender is gone.
    pub fn is_closed(&self) -> bool {
        self.signal.senders_gone.load(Ordering::Acquire)
    }

    /// Whether the lane for `class` has no room left.
    pub fn is_full(&self, class: DispatchClass) -> bool {
        self.room[lane(class)].available_permits() == 0
    }

    /// The next event already waiting in any lane.
    pub fn try_recv(&mut self) -> Option<E> {
        self.take_next(true).map(|queued| queued.event)
    }

    async fn next_queued(&mut self, completion_open: bool) -> Option<Queued<E>> {
        loop {
            // Read before looking, so an event sent just before the last
            // sender went away is still found.
            let senders_gone = self.is_closed();
            if let Some(queued) = self.take_next(completion_open) {
                return Some(queued);
            }
            if senders_gone
                && self
                    .room
                    .iter()
                    .all(|room| room.available_permits() == self.capacity)
            {
                return None;
            }
            // Only a gated completion lane can still hold events here; the
            // caller polls again once a completion worker is free.
            self.signal.queued.notified().await;
        }
    }

    fn take_next(&mut self, completion_open: bool) -> Option<Queued<E>> {
        let lanes = &self.lanes;
        fill_heads(&mut self.heads, |lane| lanes[lane].try_recv());
        let (_, oldest) = (0..LANES)
            .filter(|&lane| completion_open || lane != COMPLETION_LANE)
            .filter_map(|lane| Some((self.heads[lane].as_ref()?.0, lane)))
            .min()?;
        let (_, queued) = self.heads[oldest].take()?;
        self.room[oldest].add_permits(1);
        Some(queued)
    }
}

/// Takes each empty lane's oldest event, pass after pass, until a pass finds
/// none. A lane looked at before an event arrived in it is looked at again
/// after any later event is found, so an event sent before a head is a head
/// too.
fn fill_heads<T>(heads: &mut [Option<T>; LANES], mut poll: impl FnMut(usize) -> Option<T>) {
    loop {
        let mut found = false;
        for (lane, head) in heads.iter_mut().enumerate() {
            if head.is_none() {
                *head = poll(lane);
                found |= head.is_some();
            }
        }
        if !found {
            return;
        }
    }
}

impl<E> Drop for DispatchReceiver<E>
where
    E: 'static,
{
    fn drop(&mut self) {
        self.signal.receiver_gone.store(true, Ordering::Release);
        for room in &self.room {
            room.add_permits(Semaphore::MAX_PERMITS - self.capacity);
        }
    }
}

/// Where the dispatcher takes its events from.
pub trait DispatchSource<E> {
    /// The next event, skipping the completion lane while `completion_open`
    /// is false. A source with a single queue ignores the flag.
    fn next(
        &mut self,
        completion_open: bool,
    ) -> impl Future<Output = Option<Queued<E>>> + MaybeSend;
}

impl<E> DispatchSource<E> for DispatchReceiver<E>
where
    E: MaybeSend + 'static,
{
    fn next(
        &mut self,
        completion_open: bool,
    ) -> impl Future<Output = Option<Queued<E>>> + MaybeSend {
        self.next_queued(completion_open)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<E> DispatchSource<E> for nuclei::sync::mpsc::Receiver<E>
where
    E: MaybeSend,
{
    async fn next(&mut self, _completion_open: bool) -> Option<Queued<E>> {
        self.recv().await.map(|event| Queued { event, lease: None })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum TestEvent {
        Request(usize),
        Completion(usize),
    }

    impl DispatchEvent for TestEvent {
        fn dispatch_class(&self) -> DispatchClass {
            match self {
                Self::Request(_) => DispatchClass::Admission,
                Self::Completion(_) => DispatchClass::Completion,
            }
        }
    }

    #[nuclei::test(threads = 0)]
    async fn a_closed_completion_lane_does_not_hide_requests() {
        let (tx, mut rx) = dispatch_queue(4);
        tx.send(TestEvent::Completion(0)).await.unwrap();
        tx.send(TestEvent::Request(1)).await.unwrap();

        let queued = rx.next(false).await.unwrap();
        assert_eq!(queued.event, TestEvent::Request(1));
        assert!(
            crate::rt::time::timeout(Duration::from_millis(50), rx.next(false))
                .await
                .is_err(),
            "a gated completion must wait, not end the stream"
        );
        assert_eq!(rx.next(true).await.unwrap().event, TestEvent::Completion(0));
    }

    #[nuclei::test(threads = 0)]
    async fn a_full_lane_waits_only_for_its_own_class() {
        let (tx, mut rx) = dispatch_queue(1);
        tx.send(TestEvent::Completion(0)).await.unwrap();
        assert!(rx.is_full(DispatchClass::Completion));

        crate::rt::time::timeout(Duration::from_secs(1), tx.send(TestEvent::Request(1)))
            .await
            .expect("a full completion lane must not hold up a request")
            .unwrap();
        assert_eq!(rx.recv().await, Some(TestEvent::Completion(0)));
        assert_eq!(rx.recv().await, Some(TestEvent::Request(1)));
    }

    #[nuclei::test(threads = 0)]
    async fn dropping_the_receiver_fails_waiting_and_later_sends() {
        let (tx, rx) = dispatch_queue(1);
        tx.send(TestEvent::Request(0)).await.unwrap();
        let waiting = {
            let tx = tx.clone();
            crate::rt::spawn(async move { tx.send(TestEvent::Request(1)).await })
        };
        crate::rt::yield_now().await;
        drop(rx);

        assert_eq!(waiting.join().await.unwrap(), Err(TestEvent::Request(1)));
        assert_eq!(
            tx.send(TestEvent::Request(2)).await,
            Err(TestEvent::Request(2))
        );
    }

    #[nuclei::test(threads = 0)]
    async fn open_lanes_keep_send_order() {
        let (tx, mut rx) = dispatch_queue(4);
        tx.send(TestEvent::Completion(0)).await.unwrap();
        tx.send(TestEvent::Request(1)).await.unwrap();
        tx.send(TestEvent::Completion(2)).await.unwrap();

        assert_eq!(rx.recv().await, Some(TestEvent::Completion(0)));
        assert_eq!(rx.recv().await, Some(TestEvent::Request(1)));
        assert_eq!(rx.recv().await, Some(TestEvent::Completion(2)));
    }

    #[test]
    fn an_event_sent_into_a_lane_already_looked_at_is_still_a_head() {
        let admission = lane(DispatchClass::Admission);
        let completion = lane(DispatchClass::Completion);
        let mut lanes: [std::collections::VecDeque<u64>; LANES] = Default::default();
        let mut heads = [None; LANES];
        let mut sent = false;
        fill_heads(&mut heads, |polled| {
            let head = lanes[polled].pop_front();
            if polled == admission && !sent {
                // Both sent after the admission lane was looked at and before
                // the completion lane is.
                lanes[admission].push_back(0);
                lanes[completion].push_back(1);
                sent = true;
            }
            head
        });
        assert_eq!(heads[admission], Some(0));
        assert_eq!(heads[completion], Some(1));
    }

    #[nuclei::test(threads = 0)]
    async fn the_receiver_ends_once_every_sender_is_gone_and_lanes_are_empty() {
        let (tx, mut rx) = dispatch_queue(4);
        tx.send(TestEvent::Completion(0)).await.unwrap();
        drop(tx);
        assert!(rx.is_closed());
        assert_eq!(rx.recv().await, Some(TestEvent::Completion(0)));
        assert_eq!(rx.recv().await, None);
    }
}
