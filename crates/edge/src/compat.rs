//! The one way into a library that needs a tokio runtime and lets us supply
//! neither its sockets nor its executor: iroh, and the vera.rs clients.
//!
//! The library's futures run where they are awaited, on nuclei, inside the
//! context of a tokio runtime that async-compat keeps on threads of its own:
//! that runtime drives the library's sockets and timers and runs the tasks
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

/// Starts async-compat's tokio runtime on the calling thread, unless it runs
/// already. Its threads take this thread's CPUs, and it sizes itself from
/// them: started on a network shard, it would be one thread on one CPU.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn start_runtime() {
    futures_lite::future::block_on(Compat::new(async {}));
}

/// Starts futures-timer's one thread, which runs libp2p's timers, unless it
/// runs already: it takes the calling thread's CPUs.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn start_timer() {
    drop(futures_timer::Delay::new(std::time::Duration::ZERO));
}
