//! Helpers shared by more than one benchmark target, and the process
//! allocator every target runs on ([`allocator`]).
//!
//! Only helpers that are byte-identical at every call site live here. A helper
//! that differs between targets stays with its target: for runtimes in
//! particular, the worker count is part of what a benchmark measures, so each
//! target names its own.

#![allow(dead_code)]

mod allocator;
pub mod sift;
pub mod vector;

use std::sync::atomic::{AtomicBool, Ordering};

use criterion::async_executor::AsyncExecutor;
use regolith::sync::OnceCell;

/// A nuclei runtime a benchmark drives its futures from, and criterion's executor for them.
pub struct Runtime(nuclei::Runtime);

impl Runtime {
    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: std::future::Future,
    {
        self.0.block_on(future)
    }
}

impl AsyncExecutor for &Runtime {
    fn block_on<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        Runtime::block_on(self, future)
    }
}

/// A runtime of `threads` workers. Zero runs every task on the thread inside
/// `block_on`.
pub fn runtime(threads: usize) -> Runtime {
    Runtime(
        nuclei::Runtime::builder()
            .worker_threads(threads)
            .build()
            .expect("a nuclei runtime"),
    )
}

/// Built once per process and reused. `Runtime::new()` is multi-threaded across
/// all available cores.
pub fn shared_runtime() -> &'static Runtime {
    static RUNTIME: OnceCell<Runtime> = OnceCell::new();
    static CLAIMED: AtomicBool = AtomicBool::new(false);
    // The runtime spawns threads, so exactly one caller builds it; the rest wait for it.
    if !CLAIMED.swap(true, Ordering::AcqRel) {
        let _ = RUNTIME.set(owned_runtime());
    }
    loop {
        if let Some(runtime) = RUNTIME.get() {
            return runtime;
        }
        std::thread::yield_now();
    }
}

/// A fresh multi-threaded runtime per call, for targets that build one outside
/// the measured region and hand it to criterion.
pub fn owned_runtime() -> Runtime {
    Runtime(nuclei::Runtime::new().expect("a nuclei runtime"))
}

/// The positive count the environment variable `name` holds, else `default`.
pub fn positive(name: &str, default: usize) -> usize {
    let value = std::env::var(name).map_or(default, |s| s.parse().expect(name));
    assert!(value > 0, "{name} must be positive");
    value
}

/// The positive comma-separated counts `name` holds, else `default`'s.
pub fn counts(name: &str, default: &str) -> Vec<usize> {
    let counts: Vec<usize> = std::env::var(name)
        .unwrap_or_else(|_| default.into())
        .split(',')
        .map(|value| value.parse().expect(name))
        .collect();
    assert!(
        counts.iter().all(|count| *count > 0),
        "{name} must be positive"
    );
    counts
}

/// What `git args` prints, trimmed.
pub fn git(args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// The load guard a baseline run names in `DEFRA_BASELINE_LOAD_GUARD`.
pub fn load_guard() -> defra_perf::run_meta::LoadGuard {
    defra_perf::run_meta::load_guard(
        std::env::var_os("DEFRA_BASELINE_LOAD_GUARD")
            .as_deref()
            .map(std::path::Path::new),
    )
}
