//! A worker's loop: rounds of inbox, tasks, hook and timers; idling by
//! spinning, then parking; and stopping, which fails what is left.

use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::Waker;
use std::time::Duration;

use super::local::Local;
use super::shared::{self, WorkerShared};
use super::{drop_quietly, micros, on_worker, Tuning, CURRENT};
use crate::clock::Clock;
use crate::completion::Completion;
use crate::hook::Hook;
use crate::Error;

/// A worker's hook and when it must next be polled.
struct HookState {
    hook: Option<Box<dyn Hook>>,
    due: Option<u64>,
    budget: usize,
}

impl HookState {
    /// Polls the hook; the completions it delivered.
    fn poll(&mut self, local: &Local) -> usize {
        let Some(hook) = self.hook.as_mut() else {
            return 0;
        };
        let progress = hook.poll(self.budget);
        self.due = progress
            .wake_in
            .map(|wait| local.now().saturating_add(micros(wait)));
        progress.completed
    }
}

/// A running worker on the calling thread. Dropping it stops the worker:
/// every task it holds is dropped (failing its completion), and every
/// message it has not taken is failed.
pub(crate) struct Worker {
    local: Rc<Local>,
    hook: HookState,
    unpark: Waker,
    woken: Vec<Waker>,
}

impl Worker {
    /// Makes the calling thread `shared`'s worker. Errors if it already is
    /// one.
    pub(crate) fn new(
        shared: Arc<WorkerShared>,
        clock: Arc<dyn Clock>,
        tuning: Tuning,
        hook: Option<Box<dyn Hook>>,
    ) -> Result<Worker, Error> {
        if on_worker() {
            return Err(Error::OnWorker);
        }
        shared.bind_thread();
        let unpark = Waker::from(shared.signal.clone());
        let budget = tuning.io_budget;
        let local = Rc::new(Local::new(shared, clock, tuning));
        CURRENT.with(|current| *current.borrow_mut() = Some(local.clone()));
        Ok(Worker {
            local,
            hook: HookState {
                hook,
                due: None,
                budget,
            },
            unpark,
            woken: Vec::new(),
        })
    }

    pub(crate) fn shared(&self) -> &Arc<WorkerShared> {
        &self.local.shared
    }

    /// Spawns a daemon from outside any task (the pool's own periodic jobs).
    pub(crate) fn spawn_daemon<F>(&self, name: &'static str, future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        let _ = self.local.spawn(name, future, true);
    }

    /// Spawns a task from outside any task (the driving thread in driven
    /// mode).
    pub(crate) fn spawn<F>(&self, future: F) -> Result<Completion<F::Output>, Error>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.local.spawn(std::any::type_name::<F>(), future, false)
    }

    /// One round: a batch from the inboxes, a batch of ready tasks with the
    /// hook polled between them, then due timers. Returns how much it did;
    /// zero means the worker may idle.
    pub(crate) fn turn(&mut self) -> usize {
        let local = &self.local;
        let mut done = 0;
        for i in 0..local.tuning.inbox_batch {
            match local.shared.take(i % 2 == 1) {
                Some(shared::Message::Job(job)) => local.start(job),
                Some(shared::Message::Shutdown) => local.draining.set(true),
                None => break,
            }
            done += 1;
        }
        let mut polled = 0;
        while polled < local.tuning.run_batch {
            let Some(key) = local.shared.signal.pop() else {
                break;
            };
            if local.poll(key) {
                polled += 1;
                done += self.hook.poll(local);
            }
        }
        if polled == 0 {
            done += self.hook.poll(local);
        }
        done += polled;
        if !local.timers.borrow().is_empty() {
            let now = local.now();
            local.timers.borrow_mut().fire(now, &mut self.woken);
            done += self.woken.len();
            self.woken.drain(..).for_each(Waker::wake);
        }
        local.shared.heartbeat.idle();
        done
    }

    /// Whether the worker should stop: told to abandon, or told to shut down
    /// and nothing admitted is left.
    pub(crate) fn finished(&self) -> bool {
        self.local.shared.abandon.load(Ordering::Acquire)
            || (self.local.draining.get() && self.local.drained())
    }

    /// Something is ready to run now.
    pub(crate) fn has_work(&self) -> bool {
        self.local.shared.signal.has_ready()
            || self.local.shared.has_messages()
            || self.local.timers_due()
    }

    /// When the worker must run next with nothing else waking it: its
    /// earliest timer or the hook's deadline, on the pool's clock.
    pub(crate) fn next_due(&self) -> Option<u64> {
        let timer = self.local.timers.borrow_mut().next_due();
        match (timer, self.hook.due) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// How long until [`next_due`](Self::next_due), from now.
    pub(crate) fn wait_for(&self) -> Option<Duration> {
        self.next_due()
            .map(|due| Duration::from_micros(due.saturating_sub(self.local.now())))
    }

    /// Runs until told to stop.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn run(mut self) {
        loop {
            let done = self.turn();
            if self.finished() {
                break;
            }
            if done == 0 {
                self.idle();
            }
        }
    }

    /// The worker is about to rest: marks it parked, so the next push, wake
    /// or hook completion wakes it, and hands the hook its waker. Returns
    /// `false`, leaving it running, when work arrived meanwhile.
    pub(crate) fn rest(&mut self) -> bool {
        let signal = &self.local.shared.signal;
        signal.parking();
        if self.has_work() || self.finished() {
            signal.running();
            return false;
        }
        if let Some(hook) = self.hook.hook.as_mut() {
            hook.idle(&self.unpark);
        }
        true
    }

    /// Nothing to do: spin briefly, then park until a push, a wake, a timer
    /// or the hook needs the worker.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn idle(&mut self) {
        let spin = micros(self.local.tuning.idle_spin);
        if spin > 0 {
            let until = self.local.now().saturating_add(spin);
            let mut round = 0u32;
            loop {
                if self.has_work() {
                    return;
                }
                round = round.wrapping_add(1);
                if round.is_multiple_of(64) {
                    if self.hook.poll(&self.local) > 0 {
                        return;
                    }
                    if self.local.now() >= until {
                        break;
                    }
                }
                std::hint::spin_loop();
            }
        }
        if !self.rest() {
            return;
        }
        match self.wait_for() {
            Some(wait) => std::thread::park_timeout(wait),
            None => std::thread::park(),
        }
        self.local.shared.signal.running();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let local = &self.local;
        local.close();
        local.shared.heartbeat.idle();
        local.shared.exit();
        drop_quietly(self.hook.hook.take());
        CURRENT.with(|current| *current.borrow_mut() = None);
    }
}
