use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use super::*;

#[derive(Default)]
struct WakeCount(AtomicUsize);

impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn overflow_wakes_reader_that_drained_before_loss_was_recorded() {
    workers::block_on(async {
        let bus = ChannelBus::with_config(ChannelBusConfig::new().with_event_buffer_size(1));
        let mut subscription = bus.subscribe_closing_on_overflow(&[EventName::Merge]);
        bus.publish(Message::merge());
        let subscriber = bus
            .subscribers
            .load()
            .get(&subscription.id())
            .unwrap()
            .clone();

        assert!(matches!(
            subscriber.sender.try_send(Message::merge()),
            Err(TrySendError::Full(_))
        ));
        assert!(subscription.recv().await.is_some());
        assert_eq!(subscription.check_and_reset_dropped(), 0);

        let wakes = Arc::new(WakeCount::default());
        let waker = Waker::from(wakes.clone());
        let mut context = Context::from_waker(&waker);
        let mut waiting = Box::pin(subscription.recv());
        assert!(matches!(waiting.as_mut().poll(&mut context), Poll::Pending));

        assert_eq!(subscriber.record_overflow(), 0);
        assert!(wakes.0.load(Ordering::Relaxed) > 0);
        assert!(matches!(
            waiting.as_mut().poll(&mut context),
            Poll::Ready(None)
        ));
    })
    .unwrap();
}

#[test]
fn ordinary_raw_subscription_remains_open_after_overflow() {
    workers::block_on(async {
        let bus = ChannelBus::with_config(ChannelBusConfig::new().with_event_buffer_size(1));
        let mut subscription = bus.subscribe(&[EventName::Merge]);
        bus.publish(Message::merge());
        bus.publish(Message::merge());
        assert!(subscription.recv().await.is_some());
        assert!(matches!(
            subscription.try_recv(),
            Err(crate::TryRecvError::Empty)
        ));
        assert_eq!(subscription.check_and_reset_dropped(), 1);
    })
    .unwrap();
}
