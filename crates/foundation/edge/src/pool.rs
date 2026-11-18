//! The edge's timers serving the worker pool: the pool's stuck-poll monitor
//! runs here, so no worker spends polls watching the others.

use std::sync::Arc;

use crate::handle::Edge;

impl Edge {
    /// A driver for [`workers::Config::monitor`]: runs the pool's stuck-poll
    /// scan every period on this runtime's timer, until the pool is gone or
    /// this runtime stops accepting work.
    ///
    /// ```no_run
    /// # fn demo(edge: &edge::Edge) -> Result<(), workers::Error> {
    /// let pool = workers::Pool::start(workers::Config {
    ///     monitor: Some(edge.pool_monitor()),
    ///     ..workers::Config::default()
    /// })?;
    /// # let _ = pool;
    /// # Ok(())
    /// # }
    /// ```
    pub fn pool_monitor(&self) -> workers::MonitorDriver {
        let edge = self.clone();
        Arc::new(move |monitor: workers::Monitor| {
            let timer = edge.clone();
            // Detached: it ends with the pool or with the runtime.
            drop(edge.spawn(async move {
                loop {
                    timer.sleep(monitor.period()).await;
                    if timer.is_closed() || monitor.scan().is_none() {
                        return;
                    }
                }
            }));
        })
    }
}

/// A one-worker pool the browser's event loop drives (wasm32): the page's
/// thread is the worker. Whenever the pool has work, a task on the event
/// loop runs the worker's rounds until it is idle (a bounded slice of rounds
/// before it yields to the page), and a browser timer wakes it for its next
/// deadline. The worker measures its own polls against the stuck limit.
#[cfg(target_arch = "wasm32")]
pub fn page_pool(config: workers::Config) -> Result<workers::Pool, workers::Error> {
    page::start(config)
}

#[cfg(target_arch = "wasm32")]
mod page {
    use std::cell::{Cell, RefCell};
    use std::sync::Arc;
    use std::task::{Wake, Waker};

    /// Worker rounds one event-loop task runs before it yields to the page.
    pub(super) const TURNS_PER_TASK: usize = 64;

    thread_local! {
        static DRIVER: RefCell<Option<workers::Driver>> = const { RefCell::new(None) };
        /// The page's pool, handed to every client on the page.
        static POOL: RefCell<Option<workers::Pool>> = const { RefCell::new(None) };
        static SCHEDULED: Cell<bool> = const { Cell::new(false) };
    }

    /// Wakes the page's worker: queues one event-loop task to run it.
    struct PageWake;

    impl Wake for PageWake {
        fn wake(self: Arc<Self>) {
            schedule();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            schedule();
        }
    }

    fn schedule() {
        if !SCHEDULED.with(|scheduled| scheduled.replace(true)) {
            drop(n0_future::task::spawn(async { run() }));
        }
    }

    /// Runs the worker's rounds until it is idle or the slice is spent, then
    /// arranges the next run: at once while it is busy, by a timer for its
    /// next deadline otherwise.
    fn run() {
        SCHEDULED.with(|scheduled| scheduled.set(false));
        let (busy, due) = DRIVER.with(|driver| {
            let mut driver = driver.borrow_mut();
            let Some(driver) = driver.as_mut() else {
                return (false, None);
            };
            let busy = (0..TURNS_PER_TASK).all(|_| driver.turn() > 0);
            (busy, driver.next_due())
        });
        if busy {
            schedule();
        } else if let Some(due) = due {
            drop(n0_future::task::spawn(async move {
                n0_future::time::sleep(due).await;
                schedule();
            }));
        }
    }

    /// The page's monotonic clock, for a pool given none (the browser has no
    /// default the pool can read).
    #[derive(Debug)]
    struct PageClock(n0_future::time::Instant);

    impl workers::Clock for PageClock {
        fn now_micros(&self) -> u64 {
            u64::try_from(self.0.elapsed().as_micros()).unwrap_or(u64::MAX)
        }
    }

    /// The page's one worker serves every client on the page: a pool already
    /// running is shared, and only one that has stopped is replaced.
    pub(super) fn start(config: workers::Config) -> Result<workers::Pool, workers::Error> {
        let running = DRIVER.with(|driver| driver.borrow().as_ref().is_some_and(|d| !d.finished()));
        if running {
            if let Some(pool) = POOL.with(|pool| pool.borrow().clone()) {
                return Ok(pool);
            }
        }
        // A stopped driver lets go of the page's thread before a new one starts.
        DRIVER.with(|driver| drop(driver.borrow_mut().take()));
        let clock = config.clock.clone().unwrap_or_else(|| {
            let clock: Arc<dyn workers::Clock> =
                Arc::new(PageClock(n0_future::time::Instant::now()));
            clock
        });
        let config = workers::Config {
            clock: Some(clock),
            ..config
        };
        let (pool, driver) = workers::Pool::driven(config, Waker::from(Arc::new(PageWake)))?;
        DRIVER.with(|slot| *slot.borrow_mut() = Some(driver));
        POOL.with(|slot| *slot.borrow_mut() = Some(pool.clone()));
        schedule();
        Ok(pool)
    }
}
