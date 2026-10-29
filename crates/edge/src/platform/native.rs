//! The runtime natively: one nuclei runtime, built, held and dropped by an
//! owner thread of its own, so it is always dropped outside any async
//! context, whoever drops the `EdgeRuntime` and wherever.

use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::thread;

use nuclei::task::{AbortHandle, Join};
use nuclei::{Runtime, RuntimeHandle};
use regolith::sync::{Event, OnceCell};
use workers::topology::CpuSet;

pub(crate) use nuclei::time::{Instant, Sleep};

use crate::config::EdgeConfig;
use crate::error::EdgeError;
use crate::gate::Gate;
use crate::handle::{drain_gate, Drained};
use crate::shared::Shared;

/// Where network work runs: the runtime's handle, and the CPUs its threads
/// are pinned to (empty when unpinned).
#[derive(Clone)]
pub(crate) struct Rt {
    handle: RuntimeHandle,
    pub(crate) cpus: CpuSet,
}

impl Rt {
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        JoinHandle::new(self.handle.spawn(future))
    }

    pub(crate) fn blocking<F, T>(&self, work: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        JoinHandle::new(self.handle.spawn_blocking(work))
    }

    /// Drives `future` on the calling thread. Inside a nuclei task the wait
    /// nests: the thread's shard keeps running its other tasks meanwhile.
    pub(crate) fn drive<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.handle.block_on(future)
    }

    pub(crate) fn sleep_until(&self, at: Instant) -> Sleep {
        nuclei::time::sleep_until(at)
    }

    /// Polls a timer of this runtime's clock from any thread. A timer polled
    /// off the runtime's shards is fired by the runtime the thread is in, so
    /// the poll enters this one; otherwise nuclei's default runtime would
    /// start to fire it.
    pub(crate) fn poll_sleep(&self, sleep: Pin<&mut Sleep>, cx: &mut Context<'_>) -> Poll<()> {
        let _runtime = self.handle.enter();
        sleep.poll(cx)
    }
}

/// A spawned task's completion. Dropping it leaves the task running: nuclei
/// cancels a task whose handle is dropped, so the handle detaches it first.
pub(crate) struct JoinHandle<T> {
    abort: AbortHandle,
    join: Option<Join<T>>,
}

impl<T> JoinHandle<T> {
    fn new(task: nuclei::Task<T>) -> Self {
        Self {
            abort: task.abort_handle(),
            join: Some(task.join()),
        }
    }

    pub(crate) fn abort(&self) {
        self.abort.abort();
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.abort.is_finished()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, EdgeError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(join) = self.join.as_mut() else {
            return Poll::Ready(Err(EdgeError::Cancelled));
        };
        Pin::new(join).poll(cx).map(|joined| {
            joined.map_err(|error| {
                if error.is_panic() {
                    EdgeError::Panicked(error.to_string())
                } else {
                    EdgeError::Cancelled
                }
            })
        })
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            join.detach();
        }
    }
}

/// Runs `wait` on the calling thread, which it blocks. A nuclei worker
/// blocked here hands its shard to another thread of its core, so the
/// shard's other tasks go on. A panic inside `wait` comes back as the inner
/// error.
pub(crate) fn wait_here<T, W>(wait: W) -> thread::Result<T>
where
    W: FnOnce() -> T,
{
    catch_unwind(AssertUnwindSafe(wait))
}

#[derive(Default)]
struct Control {
    stop: Event,
    finished: Event,
    drained: OnceCell<Drained>,
}

/// The owner thread: it builds the runtime, waits for the stop request,
/// then drains, stops the rest and drops the runtime, in that order.
pub(crate) struct Owner {
    control: Arc<Control>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Owner {
    pub(crate) fn start(
        config: &EdgeConfig,
        gate: &Arc<Gate>,
    ) -> Result<(Arc<Shared>, Self), EdgeError> {
        let control = Arc::new(Control::default());
        let (started, starting) = std::sync::mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name(format!("{}-owner", config.thread_name))
            .spawn({
                let config = config.clone();
                let gate = Arc::clone(gate);
                let control = Arc::clone(&control);
                move || own(config, gate, control, started)
            })
            .map_err(|error| EdgeError::Start(error.to_string()))?;
        match starting.recv() {
            Ok(Ok(shared)) => Ok((
                shared,
                Self {
                    control,
                    thread: Some(thread),
                },
            )),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(EdgeError::Start(
                    "the runtime's owner thread exited while starting".to_string(),
                ))
            }
        }
    }

    pub(crate) fn request_stop(&self) {
        self.control.stop.set();
    }

    /// Waits on the calling thread until the owner dropped the runtime.
    pub(crate) fn wait(&mut self) -> Result<Drained, EdgeError> {
        let Some(thread) = self.thread.take() else {
            return self.drained();
        };
        match wait_here(|| thread.join()) {
            Ok(Ok(())) => self.drained(),
            Ok(Err(_)) | Err(_) => Err(EdgeError::Panicked(
                "the network runtime's owner thread panicked while stopping".to_string(),
            )),
        }
    }

    pub(crate) async fn finish(&mut self) -> Result<Drained, EdgeError> {
        self.request_stop();
        self.control.finished.wait().await;
        self.drained()
    }

    fn drained(&self) -> Result<Drained, EdgeError> {
        self.control
            .drained
            .get()
            .copied()
            .ok_or(EdgeError::Cancelled)
    }
}

type Started = std::sync::mpsc::SyncSender<Result<Arc<Shared>, EdgeError>>;

fn own(config: EdgeConfig, gate: Arc<Gate>, control: Arc<Control>, started: Started) {
    let cores = Arc::new(match config.threads {
        Some(threads) => workers::EdgeCores::unpinned(threads),
        None => workers::EdgeCores::reserve(),
    });
    crate::compat::start_runtime();
    crate::compat::start_timer();
    let runtime = match build(&config, &cores) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = started.send(Err(EdgeError::Start(error.to_string())));
            return;
        }
    };
    let shared = Shared::new(
        Rt {
            handle: runtime.handle(),
            cpus: cores.cpus().clone(),
        },
        config.deadline,
    );
    if started.send(Ok(Arc::clone(&shared))).is_err() {
        return;
    }
    let _finished = Finished(&control);
    tracing::info!(
        threads = cores.threads(),
        cpus = %cores.cpus(),
        name = %config.thread_name,
        "network runtime started"
    );

    runtime.block_on(control.stop.wait());
    gate.close();
    let drained = runtime.block_on(drain_gate(&gate, &shared, config.drain));
    shared.stop();
    runtime.shutdown_timeout(config.stop);

    if drained.is_complete() {
        tracing::info!("network runtime stopped");
    } else {
        tracing::warn!(
            remaining = drained.remaining,
            drain_secs = config.drain.as_secs_f64(),
            "network runtime stopped work that outlived the drain deadline"
        );
    }
    let _ = control.drained.set(drained);
}

/// Sets `finished` however `own` ends, so an awaited shutdown never hangs.
struct Finished<'a>(&'a Control);

impl Drop for Finished<'_> {
    fn drop(&mut self) {
        self.0.finished.set();
    }
}

/// Spawns balance strictly across the shards: an accept loop spawns every
/// connection it takes, and with nuclei's default margin the first 32 would
/// stay on the accepting shard, one core serving them all. On cores of its
/// own, nuclei pins each shard to one CPU of the calling thread's affinity,
/// which [`workers::EdgeCores::reserve`] set to them, and every thread that
/// starts later pins itself there. The hook keeps `cores`, so the hold
/// outlives the last thread placed through it.
fn build(config: &EdgeConfig, cores: &Arc<workers::EdgeCores>) -> std::io::Result<Runtime> {
    let mut builder = Runtime::builder()
        .thread_name(config.thread_name.clone())
        .worker_threads(cores.threads())
        .pin_workers(cores.is_pinned())
        .local_spawn_margin(0);
    let hook = config.on_thread_start.clone();
    if cores.is_pinned() || hook.is_some() {
        let cores = Arc::clone(cores);
        builder = builder.on_thread_start(move || {
            cores.pin_current_thread();
            if let Some(hook) = &hook {
                hook();
            }
        });
    }
    builder.build()
}
