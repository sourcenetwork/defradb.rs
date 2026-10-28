//! The one holder of the network runtime: it starts the runtime and shuts
//! it down in one order.

use crate::config::EdgeConfig;
use crate::error::EdgeError;
use crate::gate::Gate;
use crate::handle::{Drained, Edge};
use crate::platform::Owner;

/// The network runtime. One per process, held in one place: the program's
/// entry point, the C library's state, or a node that was given none.
///
/// Shutdown runs in one order: stop accepting (spawns are refused and
/// [`Edge::closing`] completes, so servers stop accepting connections),
/// drain admitted work for [`EdgeConfig::drain`], stop the rest, waiting
/// [`EdgeConfig::stop`] for blocking work, then drop the runtime on its own
/// owner thread, outside any async context. Dropping an `EdgeRuntime`
/// starts the same shutdown without waiting for it. Natively the runtime is
/// a nuclei runtime; on wasm32 it is the browser's event loop, where nothing
/// is dropped.
pub struct EdgeRuntime {
    edge: Edge,
    owner: Owner,
}

impl EdgeRuntime {
    /// Starts the runtime.
    pub fn start(config: EdgeConfig) -> Result<Self, EdgeError> {
        let gate = Gate::root();
        let (shared, owner) = Owner::start(&config, &gate)?;
        Ok(Self {
            edge: Edge::new(shared, gate),
            owner,
        })
    }

    /// The handle to pass to every network component.
    pub fn edge(&self) -> &Edge {
        &self.edge
    }

    /// Shuts down in order and waits on the calling thread until the
    /// runtime is gone.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn shutdown(mut self) -> Result<Drained, EdgeError> {
        self.stop_accepting();
        self.owner.wait()
    }

    /// Shuts down in order and completes once the runtime is gone. Await it
    /// anywhere but on a task of this runtime, which the shutdown stops.
    pub async fn shutdown_async(mut self) -> Result<Drained, EdgeError> {
        self.stop_accepting();
        self.owner.finish().await
    }

    fn stop_accepting(&self) {
        self.edge.gate.close();
        self.owner.request_stop();
    }
}

impl Drop for EdgeRuntime {
    fn drop(&mut self) {
        self.stop_accepting();
    }
}
