//! The single-threaded channels, semaphore and `select!` the browser build
//! runs on, driven natively.
#![allow(
    clippy::arc_with_non_send_sync,
    reason = "the owned permits take an Arc, as the native semaphore's do; these tests never leave one thread"
)]

use std::sync::Arc;

use futures_lite::future::{block_on, pending, poll_once, ready};
use p2p::local_select;
use p2p::rt::local::mpsc::error::{SendError, TryRecvError, TrySendError};
use p2p::rt::local::semaphore::TryAcquireError;
use p2p::rt::local::{mpsc, oneshot, watch, Semaphore};

#[test]
fn bounded_send_after_receiver_drop_returns_the_value() {
    let (tx, rx) = mpsc::channel(2);
    drop(rx);
    assert!(tx.is_closed());
    assert_eq!(block_on(tx.send(7)), Err(SendError(7)));
    assert_eq!(tx.try_send(8), Err(TrySendError::Closed(8)));
    assert!(block_on(tx.reserve()).is_err());
}

#[test]
fn bounded_recv_ends_once_senders_are_gone_and_the_buffer_is_empty() {
    let (tx, mut rx) = mpsc::channel(4);
    let other = tx.clone();
    block_on(tx.send(1)).unwrap();
    drop(tx);
    block_on(other.send(2)).unwrap();
    drop(other);
    assert!(rx.is_closed());
    assert_eq!(block_on(rx.recv()), Some(1));
    assert_eq!(block_on(rx.recv()), Some(2));
    assert_eq!(block_on(rx.recv()), None);
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn try_recv_tells_empty_from_disconnected() {
    let (tx, mut rx) = mpsc::channel::<u8>(1);
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    drop(tx);
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn full_channel_refuses_try_send_and_wakes_a_waiting_sender() {
    let (tx, mut rx) = mpsc::channel(1);
    tx.try_send(1).unwrap();
    assert_eq!(tx.capacity(), 0);
    assert_eq!(tx.try_send(2), Err(TrySendError::Full(2)));
    let mut waiting = Box::pin(tx.send(2));
    assert!(block_on(poll_once(&mut waiting)).is_none());
    assert_eq!(rx.try_recv(), Ok(1));
    assert_eq!(block_on(waiting), Ok(()));
    assert_eq!(rx.try_recv(), Ok(2));
    assert_eq!(tx.capacity(), 1);
}

#[test]
fn closing_the_receiver_fails_a_waiting_sender_but_keeps_the_queue() {
    let (tx, mut rx) = mpsc::channel(1);
    tx.try_send(1).unwrap();
    let mut waiting = Box::pin(tx.send(2));
    assert!(block_on(poll_once(&mut waiting)).is_none());
    rx.close();
    assert_eq!(block_on(waiting), Err(SendError(2)));
    assert_eq!(block_on(rx.recv()), Some(1));
    assert_eq!(block_on(rx.recv()), None);
}

#[test]
fn a_permit_holds_a_slot_and_frees_it_on_drop() {
    let (tx, mut rx) = mpsc::channel(1);
    let permit = block_on(tx.reserve()).unwrap();
    assert_eq!(tx.capacity(), 0);
    assert!(matches!(tx.try_reserve(), Err(TrySendError::Full(()))));
    permit.send(5);
    assert_eq!(rx.try_recv(), Ok(5));
    let unused = tx.try_reserve().unwrap();
    assert_eq!(tx.capacity(), 0);
    drop(unused);
    assert_eq!(tx.capacity(), 1);
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[test]
fn weak_sender_upgrades_only_while_a_sender_lives() {
    let (tx, mut rx) = mpsc::channel::<u8>(1);
    let weak = tx.downgrade();
    let upgraded = weak.upgrade().unwrap();
    drop(tx);
    assert!(weak.upgrade().is_some());
    drop(upgraded);
    assert!(weak.upgrade().is_none());
    assert_eq!(block_on(rx.recv()), None);
}

#[test]
fn sender_closed_completes_when_the_receiver_drops() {
    let (tx, rx) = mpsc::channel::<u8>(1);
    let mut closed = Box::pin(tx.closed());
    assert!(block_on(poll_once(&mut closed)).is_none());
    drop(rx);
    block_on(closed);
}

#[test]
fn unbounded_channel_sends_without_waiting_and_reports_a_gone_receiver() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    for n in 0..100 {
        tx.send(n).unwrap();
    }
    assert_eq!(block_on(rx.recv()), Some(0));
    drop(rx);
    assert!(tx.is_closed());
    assert_eq!(tx.send(1), Err(SendError(1)));
    block_on(tx.closed());
}

#[test]
fn unbounded_recv_wakes_on_send_and_ends_on_sender_drop() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut next = Box::pin(rx.recv());
    assert!(block_on(poll_once(&mut next)).is_none());
    tx.send("a").unwrap();
    assert_eq!(block_on(next), Some("a"));
    drop(tx);
    assert_eq!(block_on(rx.recv()), None);
}

#[test]
fn oneshot_delivers_the_value() {
    let (tx, rx) = oneshot::channel();
    assert_eq!(tx.send(3), Ok(()));
    assert_eq!(block_on(rx), Ok(3));
}

#[test]
fn oneshot_send_to_a_dropped_receiver_returns_the_value() {
    let (tx, rx) = oneshot::channel();
    drop(rx);
    assert!(tx.is_closed());
    assert_eq!(tx.send("x"), Err("x"));
}

#[test]
fn oneshot_recv_fails_when_the_sender_drops() {
    let (tx, rx) = oneshot::channel::<u8>();
    let mut rx = Box::pin(rx);
    assert!(block_on(poll_once(&mut rx)).is_none());
    drop(tx);
    let err = block_on(rx).unwrap_err();
    assert_eq!(err.to_string(), "channel closed");
}

#[test]
fn oneshot_sender_closed_completes_when_the_receiver_drops() {
    let (mut tx, rx) = oneshot::channel::<u8>();
    {
        let mut closed = Box::pin(tx.closed());
        assert!(block_on(poll_once(&mut closed)).is_none());
        drop(rx);
        block_on(closed);
    }
    block_on(tx.closed());
}

#[test]
fn watch_receivers_see_the_replaced_value() {
    let (tx, rx) = watch::channel(false);
    let mut first = rx.clone();
    let mut second = rx;
    let mut waiting = Box::pin(async move { *first.wait_for(|done| *done).await.unwrap() });
    assert!(block_on(poll_once(&mut waiting)).is_none());
    assert!(!tx.send_replace(true));
    assert!(block_on(waiting));
    assert!(*block_on(second.wait_for(|done| *done)).unwrap());
}

#[test]
fn watch_wait_for_fails_when_the_sender_drops_first() {
    let (tx, mut rx) = watch::channel(0);
    let mut waiting = Box::pin(async move { rx.wait_for(|n| *n > 0).await.map(|_| ()) });
    assert!(block_on(poll_once(&mut waiting)).is_none());
    drop(tx);
    assert!(block_on(waiting).is_err());
}

#[test]
fn semaphore_serves_waiters_in_order() {
    let semaphore = Arc::new(Semaphore::new(0));
    let mut first = Box::pin(Arc::clone(&semaphore).acquire_owned());
    let mut second = Box::pin(Arc::clone(&semaphore).acquire_owned());
    assert!(block_on(poll_once(&mut first)).is_none());
    assert!(block_on(poll_once(&mut second)).is_none());
    semaphore.add_permits(1);
    assert!(block_on(poll_once(&mut second)).is_none());
    let held = block_on(poll_once(&mut first)).unwrap().unwrap();
    semaphore.add_permits(1);
    assert!(block_on(second).is_ok());
    drop(held);
    assert_eq!(semaphore.available_permits(), 2);
}

#[test]
fn a_dropped_waiter_passes_its_turn_on() {
    let semaphore = Semaphore::new(0);
    let mut first = Box::pin(semaphore.acquire());
    let mut second = Box::pin(semaphore.acquire());
    assert!(block_on(poll_once(&mut first)).is_none());
    assert!(block_on(poll_once(&mut second)).is_none());
    semaphore.add_permits(1);
    drop(first);
    assert!(block_on(poll_once(&mut second)).unwrap().is_ok());
}

#[test]
fn try_acquire_fails_when_empty_or_others_wait() {
    let semaphore = Semaphore::new(1);
    let permit = semaphore.try_acquire().unwrap();
    assert_eq!(
        semaphore.try_acquire().unwrap_err(),
        TryAcquireError::NoPermits
    );
    drop(permit);
    let mut waiting = Box::pin(semaphore.acquire());
    let held = semaphore.try_acquire().unwrap();
    assert!(block_on(poll_once(&mut waiting)).is_none());
    drop(held);
    assert_eq!(
        semaphore.try_acquire().unwrap_err().to_string(),
        "no permits available"
    );
    assert!(block_on(waiting).is_ok());
}

#[test]
fn try_acquire_owned_takes_from_a_shared_semaphore() {
    let semaphore = Arc::new(Semaphore::new(1));
    let permit = Arc::clone(&semaphore).try_acquire_owned().unwrap();
    assert!(Arc::clone(&semaphore).try_acquire_owned().is_err());
    drop(permit);
    assert_eq!(semaphore.available_permits(), 1);
}

#[test]
fn forget_keeps_a_permit_out_and_add_permits_puts_more_in() {
    let semaphore = Semaphore::new(2);
    block_on(semaphore.acquire()).unwrap().forget();
    assert_eq!(semaphore.available_permits(), 1);
    let owned = Arc::new(Semaphore::new(1));
    block_on(Arc::clone(&owned).acquire_owned())
        .unwrap()
        .forget();
    assert_eq!(owned.available_permits(), 0);
    owned.add_permits(3);
    assert_eq!(owned.available_permits(), 3);
}

#[test]
fn select_runs_the_first_ready_branch() {
    let value = block_on(async {
        local_select! {
            n = pending::<u8>() => n,
            n = ready(2u8) => n + 1,
        }
    });
    assert_eq!(value, 3);
}

#[test]
fn select_skips_a_branch_whose_pattern_rejects_the_output() {
    let (tx, mut rx) = mpsc::unbounded_channel::<Option<u8>>();
    tx.send(None).unwrap();
    tx.send(Some(4)).unwrap();
    let value = block_on(async {
        local_select! {
            Some(Some(n)) = rx.recv() => n,
            else => 0,
        }
    });
    assert_eq!(value, 0);
}

#[test]
fn select_preconditions_disable_branches() {
    let value = block_on(async {
        local_select! {
            n = ready(1u8), if false => n,
            n = ready(2u8), if true => n,
        }
    });
    assert_eq!(value, 2);
}

#[test]
fn select_takes_else_when_every_branch_is_disabled() {
    let value = block_on(async {
        local_select! {
            n = ready(1u8), if false => n,
            else => 9,
        }
    });
    assert_eq!(value, 9);
}

#[test]
fn select_without_else_panics_when_every_branch_is_disabled() {
    let outcome = std::panic::catch_unwind(|| {
        block_on(async {
            local_select! {
                n = ready(1u8), if false => n,
            }
        })
    });
    assert!(outcome.is_err());
}

#[test]
fn biased_select_prefers_the_first_written_ready_branch() {
    for _ in 0..8 {
        let value = block_on(async {
            local_select! {
                biased;
                n = ready(1u8) => n,
                n = ready(2u8) => n,
            }
        });
        assert_eq!(value, 1);
    }
}

#[test]
fn fair_select_rotates_its_starting_branch() {
    let mut seen = [false; 2];
    for _ in 0..4 {
        let value = block_on(async {
            local_select! {
                n = ready(0usize) => n,
                n = ready(1usize) => n,
            }
        });
        seen[value] = true;
    }
    assert_eq!(seen, [true, true]);
}

#[test]
fn select_handlers_can_break_and_use_what_the_futures_borrowed() {
    let mut log = Vec::new();
    block_on(async {
        loop {
            local_select! {
                () = ready(()) => {
                    log.push(1);
                    break;
                }
                () = pending::<()>() => {}
            }
        }
    });
    assert_eq!(log, [1]);
}
