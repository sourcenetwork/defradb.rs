//! One transaction's task on its owner, and the messages it takes.
//!
//! The task holds the transaction by value for its whole life and takes the
//! messages of its mailbox one at a time, in the order they arrived: a
//! request runs only after every request queued before it has finished, and
//! the end of the transaction (commit, discard, abandon, an idle sweep)
//! comes after everything queued before that end. Nothing else ever touches
//! the transaction, so it needs no lock.
//!
//! Between requests the transaction waits in a one-place slot it shares with
//! its table entry, so an abandon from any thread drops an idle transaction
//! at once; a request finding the slot empty was abandoned. A commit or
//! discard takes the entry out of the table before it is queued, so a
//! request either is queued before the end or finds nothing.

use std::sync::{Arc, Weak};
use std::time::Duration;

use defra_core::thread_bounds::MaybeSend;
use futures::future::LocalBoxFuture;
use regolith::sync::{ArrayQueue, UnboundedReceiver, UnboundedSender};
use web_time::Instant;

use super::idle::expired;
use super::{Shared, TxnId};
use crate::txn::fence::Landing;

/// A request's work on the transaction, given it by value: it puts the
/// transaction back in its idle slot before its answer goes out.
#[cfg(not(target_family = "wasm"))]
pub(super) type Work<T> =
    Box<dyn for<'slot> FnOnce(T, &'slot ArrayQueue<T>) -> LocalBoxFuture<'slot, ()> + Send>;
/// A request's work on the transaction, given it by value: it puts the
/// transaction back in its idle slot before its answer goes out.
#[cfg(target_family = "wasm")]
pub(super) type Work<T> =
    Box<dyn for<'slot> FnOnce(T, &'slot ArrayQueue<T>) -> LocalBoxFuture<'slot, ()>>;

/// The work that ends the transaction, given it by value.
#[cfg(not(target_family = "wasm"))]
pub(super) type Last<T> = Box<dyn FnOnce(T) -> LocalBoxFuture<'static, ()> + Send>;
/// The work that ends the transaction, given it by value.
#[cfg(target_family = "wasm")]
pub(super) type Last<T> = Box<dyn FnOnce(T) -> LocalBoxFuture<'static, ()>>;

pub(super) enum Message<T> {
    /// A request.
    Run(Work<T>),
    /// The transaction's end: commit or discard. `begun` says it began when
    /// it was handed over, no request running the transaction then; one that
    /// had to wait for a request begins when the owner takes it, and a caller
    /// that stopped waiting before that gets an abandon instead. Dropping the
    /// `fence`, once the end is done or the message is dropped unrun, lets
    /// go of what a caller that stopped waiting fenced.
    Finish {
        last: Last<T>,
        fence: Landing,
        begun: bool,
    },
    /// The caller let go of the transaction without finishing it.
    Abandon,
    /// End the transaction if no request has finished for at least
    /// this long; the reply says whether it ended.
    Expire {
        max_idle: Duration,
        reply: Option<UnboundedSender<bool>>,
    },
}

/// A request's work, its signature fixed for every lifetime of the slot.
pub(super) fn work<T, F>(work: F) -> Work<T>
where
    F: for<'slot> FnOnce(T, &'slot ArrayQueue<T>) -> LocalBoxFuture<'slot, ()>
        + MaybeSend
        + 'static,
{
    Box::new(work)
}

/// Runs the transaction waiting in `idle`, named `id`, on worker `worker`
/// until a message ends it, it is abandoned, or every sender of its mailbox
/// is gone.
/// Dropping `txn` is how it ends without a commit: its holder discards it
/// then. The task holds its registry weakly, so a dropped registry ends every
/// transaction it owned.
pub(super) async fn own<T, K>(
    idle: Arc<ArrayQueue<T>>,
    id: TxnId,
    worker: usize,
    mailbox: UnboundedReceiver<Message<T>>,
    shared: Weak<Shared<T, K>>,
) where
    T: 'static,
    K: Clone + PartialEq + 'static,
{
    let _drain = Drain(&mailbox);
    let mut active = Instant::now();
    // Out of the table, then emptied: a request is sent on this worker, so
    // it is queued before this or finds no transaction. A message left in the
    // mailbox would keep its reply open until kovan reclaims the channel, so
    // each is dropped here and its sender hears the transaction is gone.
    // Says whether this task took the entry out of the table.
    let leave = || {
        let took = shared
            .upgrade()
            .is_some_and(|shared| shared.tables[worker].take(id).is_some());
        while mailbox.try_recv().is_some() {}
        took
    };
    // Whether the loop ended on the message of whoever took the entry out of
    // the table: a commit, a discard or an abandon.
    let mut taker_seen = false;
    while let Some(message) = mailbox.recv_async().await {
        match message {
            Message::Run(work) => {
                let Some(txn) = idle.pop() else { break };
                work(txn, &idle).await;
                active = Instant::now();
            }
            Message::Finish { last, fence, begun } => {
                taker_seen = true;
                if !begun && fence.abandoned() {
                    break;
                }
                let Some(txn) = idle.pop() else { break };
                leave();
                last(txn).await;
                // Landed or failed: only now may what began after its caller
                // stopped waiting go ahead.
                drop(fence);
                return;
            }
            Message::Abandon => {
                taker_seen = true;
                break;
            }
            Message::Expire { max_idle, reply } => {
                let stale = expired(active, Instant::now(), max_idle);
                if let Some(reply) = reply {
                    let _ = reply.try_send(stale);
                }
                if stale {
                    break;
                }
            }
        }
    }
    // Ended before the mailbox empties, so a request still queued behind the
    // end is told "not found" only once the transaction is already gone.
    drop(idle.pop());
    if !leave() && !taker_seen {
        // Whoever took the entry sends its end right after taking it: it is
        // dropped here, so its reply and its commit's fence are let go now,
        // not when the channel is reclaimed.
        while let Some(message) = mailbox.recv_async().await {
            if matches!(message, Message::Finish { .. } | Message::Abandon) {
                break;
            }
        }
    }
}

/// Empties the mailbox when the task ends, however it ends: a worker that
/// stops drops the task mid-wait, and each message still queued lets go of
/// its reply and its commit's fence then, not when the channel is reclaimed.
struct Drain<'mailbox, T>(&'mailbox UnboundedReceiver<Message<T>>)
where
    T: 'static;

impl<T> Drop for Drain<'_, T>
where
    T: 'static,
{
    fn drop(&mut self) {
        while self.0.try_recv().is_some() {}
    }
}
