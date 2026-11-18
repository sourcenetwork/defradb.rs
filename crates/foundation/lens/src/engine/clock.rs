//! What advances an engine's epoch. A call's tick budget and its yields both
//! count the ticks, so a call past its budget traps instead of running forever.
//!
//! The source is a [`TickSource`]: the engine hands it an [`EpochTick`] and the
//! sandbox's period, and holds the guard it returns while any clone of the
//! engine lives. On a worker pool the ticks are a periodic job of its deadline
//! queue ([`PoolTicks`]); without one a thread of its own ticks
//! ([`ThreadTicks`]), started by the engine's first call, so a database that
//! never runs a lens starts no thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use regolith::sync::OnceCell;
use tracing::error;
use wasmtime::Engine;

use crate::Result;

/// Stops a tick source when dropped.
pub type TickGuard = Box<dyn Ticking>;

/// What a [`TickSource`] hands back: its ticks run while it is held.
pub trait Ticking: Send + Sync {
    /// A call is about to run on the engine. A source that ticks on demand
    /// starts here; an error fails the call.
    fn calling(&self) -> Result<()> {
        Ok(())
    }
}

/// Advances a lens engine's epoch every period until its guard is dropped.
pub trait TickSource: Send + Sync {
    /// Starts calling [`EpochTick::tick`] every `period`.
    fn start(&self, tick: EpochTick, period: Duration) -> Result<TickGuard>;
}

/// The handle a [`TickSource`] advances one engine's epoch through.
pub struct EpochTick(pub(super) Engine);

impl EpochTick {
    /// Advances the epoch by one tick: every call running on the engine then
    /// yields, and a call out of ticks traps.
    pub fn tick(&self) {
        self.0.increment_epoch();
    }
}

/// Ticks from a background job on a worker of the pool: one timer on that
/// worker's deadline queue per period, and no thread of its own.
pub struct PoolTicks(pub workers::Pool);

impl TickSource for PoolTicks {
    fn start(&self, tick: EpochTick, period: Duration) -> Result<TickGuard> {
        let job = workers::spawn_background(Some(&self.0), "lens::epoch", move || async move {
            let Err(error) = workers::every(period, || {
                tick.tick();
                async {}
            })
            .await;
            error!(%error, "lens epoch ticks stopped: they run only on a worker");
        });
        Ok(Box::new(StopOnDrop(job)))
    }
}

/// Stops the tick job when the engine lets go of it.
struct StopOnDrop(workers::Background);

impl Ticking for StopOnDrop {}

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Ticks on a thread of its own, parked between ticks.
pub(crate) struct ThreadTicks;

impl TickSource for ThreadTicks {
    fn start(&self, tick: EpochTick, period: Duration) -> Result<TickGuard> {
        Ok(Box::new(TickThread::new(tick, period)))
    }
}

/// The thread that ticks one engine, spawned by the engine's first call.
struct TickThread {
    tick: EpochTick,
    period: Duration,
    started: AtomicBool,
    stop: Arc<AtomicBool>,
    worker: OnceCell<JoinHandle<()>>,
}

impl TickThread {
    fn new(tick: EpochTick, period: Duration) -> Self {
        Self {
            tick,
            period,
            started: AtomicBool::new(false),
            stop: Arc::new(AtomicBool::new(false)),
            worker: OnceCell::new(),
        }
    }
}

impl Ticking for TickThread {
    fn calling(&self) -> Result<()> {
        if self.started.load(Ordering::Acquire) || self.started.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let tick = EpochTick(self.tick.0.clone());
        let (stop, period) = (Arc::clone(&self.stop), self.period);
        let worker = thread::Builder::new()
            .name("defradb-lens-epoch".into())
            .spawn(move || loop {
                thread::park_timeout(period);
                if stop.load(Ordering::Acquire) {
                    break;
                }
                tick.tick();
            })
            .inspect_err(|_| self.started.store(false, Ordering::Release))?;
        // Only the call that flipped `started` spawns, so the cell is empty.
        let _ = self.worker.set(worker);
        Ok(())
    }
}

impl Drop for TickThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = std::mem::replace(&mut self.worker, OnceCell::new()).into_inner() {
            worker.thread().unpark();
            if worker.join().is_err() {
                error!("lens epoch clock thread panicked; lens time limits stopped at the panic");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tick_thread_starts_with_the_first_call_and_only_once() {
        let engine = Engine::new(&super::super::shared::base_config()).unwrap();
        let ticks = TickThread::new(EpochTick(engine), Duration::from_millis(1));
        assert!(ticks.worker.get().is_none(), "no call, no thread");
        ticks.calling().unwrap();
        let first = ticks.worker.get().map(|worker| worker.thread().id());
        assert!(first.is_some(), "the first call starts the thread");
        ticks.calling().unwrap();
        assert_eq!(ticks.worker.get().map(|worker| worker.thread().id()), first);
        drop(ticks);
    }
}
