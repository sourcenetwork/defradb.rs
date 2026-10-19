//! Transport events queued by dispatch class, so a full class waits alone.
//!
//! One bounded lane per [`DispatchClass`]. When the completion workers are
//! busy, the dispatcher stops taking from the completion lane only; requests,
//! recovery and inline events keep flowing.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use defra_core::thread_bounds::MaybeSend;
use regolith::sync::{unbounded, Semaphore, UnboundedReceiver, UnboundedSender};

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
    let closed = Arc::new(AtomicBool::new(false));
    let pairs: [_; LANES] = std::array::from_fn(|_| unbounded::<Queued<E>>());
    let senders = pairs.each_ref().map(|(sender, _)| sender.clone());
    let sender = DispatchSender {
        shared: Arc::new(SenderShared {
            lanes: senders,
            room: room.clone(),
            closed: Arc::clone(&closed),
        }),
    };
    let receiver = DispatchReceiver {
        senders: Arc::downgrade(&sender.shared),
        lanes: pairs.map(|(_, receiver)| receiver),
        drained: [false; LANES],
        room,
        capacity,
        closed,
    };
    (sender, receiver)
}

struct SenderShared<E: 'static> {
    lanes: [UnboundedSender<Queued<E>>; LANES],
    room: [Arc<Semaphore>; LANES],
    closed: Arc<AtomicBool>,
}

/// The sending half of a [`dispatch_queue`]. Every clone feeds the same lanes.
pub struct DispatchSender<E: 'static> {
    shared: Arc<SenderShared<E>>,
}

impl<E: 'static> Clone for DispatchSender<E> {
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
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(event);
        }
        self.shared.room[lane].acquire(1).await.forget();
        // The receiver wakes every waiting sender when it goes away.
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(event);
        }
        self.shared.lanes[lane]
            .try_send(Queued { event, lease })
            .map_err(|queued| queued.event)
    }
}

/// The receiving half of a [`dispatch_queue`].
pub struct DispatchReceiver<E: 'static> {
    senders: Weak<SenderShared<E>>,
    lanes: [UnboundedReceiver<Queued<E>>; LANES],
    drained: [bool; LANES],
    room: [Arc<Semaphore>; LANES],
    capacity: usize,
    closed: Arc<AtomicBool>,
}

impl<E: 'static> DispatchReceiver<E> {
    /// The next event from any lane, inline first and completion last. `None`
    /// once every sender is gone and every lane is empty.
    pub async fn recv(&mut self) -> Option<E> {
        self.next_queued(true).await.map(|queued| queued.event)
    }

    /// Whether every sender is gone.
    pub fn is_closed(&self) -> bool {
        self.senders.strong_count() == 0
    }

    /// Whether the lane for `class` has no room left.
    pub fn is_full(&self, class: DispatchClass) -> bool {
        self.room[lane(class)].available_permits() == 0
    }

    /// The next event already waiting in any lane, inline first.
    pub fn try_recv(&mut self) -> Option<E> {
        (0..LANES).find_map(|lane| {
            let queued = self.lanes[lane].try_recv()?;
            self.room[lane].add_permits(1);
            Some(queued.event)
        })
    }

    async fn next_queued(&mut self, completion_open: bool) -> Option<Queued<E>> {
        loop {
            let [inline, admission, recovery, completion] = &self.lanes;
            let drained = self.drained;
            let (lane, received) = tokio::select! {
                biased;
                queued = inline.recv_async(), if !drained[0] => (0, queued),
                queued = admission.recv_async(), if !drained[1] => (1, queued),
                queued = recovery.recv_async(), if !drained[2] => (2, queued),
                queued = completion.recv_async(), if completion_open && !drained[COMPLETION_LANE] => {
                    (COMPLETION_LANE, queued)
                }
                else => {
                    if drained.iter().all(|done| *done) {
                        return None;
                    }
                    // Only the gated completion lane can still hold events; the
                    // caller polls again once a completion worker is free.
                    std::future::pending::<()>().await;
                    unreachable!()
                }
            };
            if let Some(queued) = self.take(lane, received) {
                return Some(queued);
            }
        }
    }

    fn take(&mut self, lane: usize, received: Option<Queued<E>>) -> Option<Queued<E>> {
        match received {
            Some(queued) => {
                self.room[lane].add_permits(1);
                Some(queued)
            }
            None => {
                self.drained[lane] = true;
                None
            }
        }
    }
}

impl<E: 'static> Drop for DispatchReceiver<E> {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
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

impl<E> DispatchSource<E> for tokio::sync::mpsc::Receiver<E>
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

    #[tokio::test]
    async fn a_closed_completion_lane_does_not_hide_requests() {
        let (tx, mut rx) = dispatch_queue(4);
        tx.send(TestEvent::Completion(0)).await.unwrap();
        tx.send(TestEvent::Request(1)).await.unwrap();

        let queued = rx.next(false).await.unwrap();
        assert_eq!(queued.event, TestEvent::Request(1));
        assert!(
            n0_future::time::timeout(Duration::from_millis(50), rx.next(false))
                .await
                .is_err(),
            "a gated completion must wait, not end the stream"
        );
        assert_eq!(rx.next(true).await.unwrap().event, TestEvent::Completion(0));
    }

    #[tokio::test]
    async fn a_full_lane_waits_only_for_its_own_class() {
        let (tx, mut rx) = dispatch_queue(1);
        tx.send(TestEvent::Completion(0)).await.unwrap();
        assert!(rx.is_full(DispatchClass::Completion));

        n0_future::time::timeout(Duration::from_secs(1), tx.send(TestEvent::Request(1)))
            .await
            .expect("a full completion lane must not hold up a request")
            .unwrap();
        assert_eq!(rx.recv().await, Some(TestEvent::Request(1)));
    }

    #[tokio::test]
    async fn dropping_the_receiver_fails_waiting_and_later_sends() {
        let (tx, rx) = dispatch_queue(1);
        tx.send(TestEvent::Request(0)).await.unwrap();
        let waiting = {
            let tx = tx.clone();
            n0_future::task::spawn(async move { tx.send(TestEvent::Request(1)).await })
        };
        tokio::task::yield_now().await;
        drop(rx);

        assert_eq!(waiting.await.unwrap(), Err(TestEvent::Request(1)));
        assert_eq!(
            tx.send(TestEvent::Request(2)).await,
            Err(TestEvent::Request(2))
        );
    }

    #[tokio::test]
    async fn the_receiver_ends_once_every_sender_is_gone_and_lanes_are_empty() {
        let (tx, mut rx) = dispatch_queue(4);
        tx.send(TestEvent::Completion(0)).await.unwrap();
        drop(tx);
        assert!(rx.is_closed());
        assert_eq!(rx.recv().await, Some(TestEvent::Completion(0)));
        assert_eq!(rx.recv().await, None);
    }
}
