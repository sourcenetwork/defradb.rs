//! The one way into a library that needs a tokio runtime and lets us supply
//! neither its sockets nor its executor: iroh, and the vera.rs clients.
//!
//! The library's futures run where they are awaited, on nuclei, inside the
//! context of a tokio runtime the edge owns, on the edge's cores: that
//! runtime drives the library's sockets and timers and runs the tasks
//! the library spawns. Nothing else of ours names tokio. Where a library
//! takes our sockets or our executor, it is given nuclei's instead of
//! coming here.

pub use async_compat::Compat;

/// `inner` (a future, or a stream speaking tokio's or futures' I/O traits)
/// run inside the tokio context the library behind it needs. The same
/// wrapper adapts a nuclei stream to tokio's I/O traits, and back.
pub fn compat<T>(inner: T) -> Compat<T> {
    Compat::new(inner)
}

/// The tokio runtime behind [`compat`]: its threads, workers and the
/// blocking threads they start later, run on every CPU the edge holds, and
/// start the placement hook like the edge's own threads do.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn runtime(
    config: &crate::EdgeConfig,
    cores: &std::sync::Arc<workers::EdgeCores>,
) -> std::io::Result<tokio::runtime::Runtime> {
    let cores = std::sync::Arc::clone(cores);
    let hook = config.on_thread_start.clone();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(cores.threads())
        .thread_name(format!("{}-tokio", config.thread_name))
        .enable_all()
        .on_thread_start(move || {
            cores.pin_current_thread();
            if let Some(hook) = &hook {
                hook();
            }
        })
        .build()
}

/// Makes `runtime` the tokio context of the calling thread for good, so a
/// [`Compat`] polled here runs in it and async-compat never starts a runtime
/// of its own, off the edge's cores. The thread's tokio context holds its own
/// clone of the handle and drops it when the thread exits, so forgetting the
/// guard keeps nothing alive past the thread.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn enter(runtime: &tokio::runtime::Handle) {
    std::mem::forget(runtime.enter());
}

/// Starts futures-timer's one thread, which runs libp2p's timers, unless it
/// runs already: it takes the calling thread's CPUs.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn start_timer() {
    drop(futures_timer::Delay::new(std::time::Duration::ZERO));
}
