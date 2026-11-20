//! The deadline an observation waits under comes first on every turn
//! (`proofs/tla/SettleBeforeObserving.tla`, `MC_SettleBeforeObserving_Red_WakeFirst`).

use std::pin::pin;
use std::time::Duration;

use super::before;

#[test]
fn a_passed_deadline_ends_a_wait_whose_future_is_ready() {
    workers::block_on(async {
        let mut passed = Box::pin(workers::delay(Duration::ZERO).unwrap());
        workers::delay(Duration::from_millis(20))
            .unwrap()
            .await
            .unwrap();
        assert!(before(passed.as_mut(), pin!(std::future::ready(())))
            .await
            .is_none());

        let mut later = Box::pin(workers::delay(Duration::from_secs(60)).unwrap());
        assert_eq!(
            before(later.as_mut(), pin!(std::future::ready(7))).await,
            Some(7)
        );
    })
    .unwrap();
}

/// The waits for an owed record rest on this: a notification sent after a
/// `Notified` is made, and before it is first polled, is not lost.
#[test]
fn a_notification_before_the_first_poll_is_not_lost() {
    workers::block_on(async {
        let notify = regolith::sync::Notify::new();
        let changed = notify.notified();
        notify.notify_waiters();
        let mut changed = pin!(changed);
        let mut later = Box::pin(workers::delay(Duration::from_secs(60)).unwrap());
        assert_eq!(before(later.as_mut(), changed.as_mut()).await, Some(()));
    })
    .unwrap();
}
