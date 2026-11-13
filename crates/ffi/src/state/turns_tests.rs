use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::Turns;

/// Whether `ticket`'s turn has come, without waiting for it.
fn ready(ticket: &super::Ticket) -> bool {
    futures_lite::future::block_on(futures_lite::future::poll_once(ticket.wait())).is_some()
}

/// `proofs/tla/TxnTurns.tla` INV_NoOvertake: a later ticket waits until the
/// earlier one is done.
#[test]
fn tickets_are_served_in_the_order_they_were_taken() {
    let turns = Turns::new();
    let first = turns.take();
    let second = turns.take();

    assert!(ready(&first));
    assert!(!ready(&second), "the second waits while the first runs");
    drop(first);
    assert!(ready(&second));
}

/// Progress: a ticket dropped before its turn is skipped once its turn comes.
#[test]
fn a_ticket_dropped_before_its_turn_is_skipped() {
    let turns = Turns::new();
    let first = turns.take();
    let dropped = turns.take();
    let third = turns.take();

    assert!(ready(&first));
    drop(dropped);
    assert!(!ready(&third), "the third still waits for the first");
    drop(first);
    assert!(ready(&third), "the dropped ticket was skipped");
}

/// Progress: a ticket dropped on its own turn, before it started, serves the
/// next one.
#[test]
fn a_ticket_dropped_on_its_turn_serves_the_next() {
    let turns = Turns::new();
    let unused = turns.take();
    let next = turns.take();

    drop(unused);
    assert!(ready(&next));
}

/// INV_OneRunning under contention: calls racing from several threads never
/// run at once, and every one of them runs.
#[test]
fn calls_from_several_threads_never_run_at_once() {
    const CALLS: usize = 64;
    let turns = Turns::new();
    let running = Arc::new(AtomicUsize::new(0));
    let ran = Arc::new(AtomicUsize::new(0));

    let threads: Vec<_> = (0..CALLS)
        .map(|call| {
            let ticket = turns.take();
            let running = Arc::clone(&running);
            let ran = Arc::clone(&ran);
            std::thread::spawn(move || {
                // Every fourth call gives up before its turn.
                if call % 4 == 3 {
                    drop(ticket);
                    return;
                }
                workers::block_on(ticket.wait()).expect("a plain thread may wait");
                assert_eq!(
                    running.fetch_add(1, Ordering::SeqCst),
                    0,
                    "two calls ran at once"
                );
                std::thread::yield_now();
                running.fetch_sub(1, Ordering::SeqCst);
                ran.fetch_add(1, Ordering::SeqCst);
                drop(ticket);
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("a call panicked");
    }
    assert_eq!(ran.load(Ordering::SeqCst), CALLS - CALLS / 4);
}
