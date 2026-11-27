//! Network runtimes for tests of components that take an [`Edge`] at
//! construction: one a test binary's tests share, and one a test runs on
//! alone with its clock paused.

#[cfg(not(target_arch = "wasm32"))]
use std::future::Future;
use std::num::NonZeroUsize;

use regolith::sync::Lazy;

use crate::{Edge, EdgeConfig, EdgeRuntime};

static RUNTIME: Lazy<EdgeRuntime> = Lazy::new(start);

fn start() -> EdgeRuntime {
    EdgeRuntime::start(EdgeConfig {
        threads: NonZeroUsize::new(2),
        thread_name: "defra-edge-test".to_string(),
        ..EdgeConfig::default()
    })
    .expect("the test network runtime starts")
}

/// A handle onto the runtime this test binary shares, started on first
/// use and stopped when the process exits.
pub fn edge() -> Edge {
    RUNTIME.edge().clone()
}

/// Runs `test` as `#[nuclei::test(start_paused = true)]` runs a test: on a
/// runtime of its own whose clock stands still until the test advances it
/// or nothing on the runtime has work. `test` is given the runtime's
/// handle, so the tasks of the components it builds run on the paused
/// clock too. The runtime stops once `test` ends.
#[cfg(not(target_arch = "wasm32"))]
pub fn paused<M, F>(test: M) -> F::Output
where
    M: FnOnce(Edge) -> F,
    F: Future,
{
    let runtime = EdgeRuntime::start(EdgeConfig {
        threads: NonZeroUsize::new(1),
        thread_name: "defra-edge-paused-test".to_string(),
        ..EdgeConfig::default()
    })
    .expect("the paused test network runtime starts");
    let edge = runtime.edge().clone();
    let output = edge
        .block_on(async {
            nuclei::time::pause();
            test(edge.clone()).await
        })
        .expect("the test runs to its end");
    let _ = runtime.shutdown();
    output
}
