//! The engine a WASM store compiles and runs its lenses on, built by the
//! first call that needs it.
//!
//! A database opens a store whether or not it ever registers a lens, and the
//! engine is the costly part of one: wasmtime's compiler, its linker and the
//! code that builds them. Building it on first use keeps all of that out of a
//! process that never runs a lens. The store's limits are checked when it is
//! made, so a store that cannot run them still fails there.

use std::sync::Arc;

use regolith::sync::OnceCell;

use crate::engine::{LensEngine, TickSource, WasmSandboxConfig};
use crate::{Error, Result};

/// One engine, built at most once and shared by every clone: a store and the
/// stores isolated from it.
#[derive(Clone)]
pub(super) struct SharedEngine(Arc<Cell>);

struct Cell {
    engine: OnceCell<LensEngine>,
    build: Box<dyn Fn() -> Result<LensEngine> + Send + Sync>,
}

impl SharedEngine {
    /// An engine to be built under `sandbox`, its epoch advanced by `ticks`.
    pub(super) fn new<T>(sandbox: Option<WasmSandboxConfig>, ticks: T) -> Result<Self>
    where
        T: TickSource + 'static,
    {
        if let Some(sb) = &sandbox {
            sb.validate()?;
        }
        Ok(Self(Arc::new(Cell {
            engine: OnceCell::new(),
            build: Box::new(move || LensEngine::new(sandbox.clone(), &ticks)),
        })))
    }

    /// The engine, built now if no call has needed it before. Callers that
    /// race to build it each build one; one is kept and the others dropped.
    pub(super) fn get(&self) -> Result<&LensEngine> {
        if let Some(engine) = self.0.engine.get() {
            return Ok(engine);
        }
        // Whichever build was published first is the engine; a losing one,
        // its ticks included, is dropped here.
        let _ = self.0.engine.set((self.0.build)()?);
        self.0
            .engine
            .get()
            .ok_or_else(|| Error::WasmLoad("the WASM engine was not kept".into()))
    }

    /// Whether the engine has been built.
    #[cfg(test)]
    pub(super) fn is_built(&self) -> bool {
        self.0.engine.get().is_some()
    }

    /// The wasmtime engine, built if it was not.
    #[cfg(test)]
    pub(super) fn engine(&self) -> &wasmtime::Engine {
        self.get().expect("the test engine builds").engine()
    }

    /// The engine's ticks, built if they were not.
    #[cfg(test)]
    pub(super) fn ticks_handle(&self) -> Option<std::sync::Weak<dyn crate::engine::Ticking>> {
        self.get().expect("the test engine builds").ticks_handle()
    }

    /// A call on the engine, built if it was not.
    #[cfg(test)]
    pub(super) fn call<S>(
        &self,
        pipeline: crate::engine::Pipeline,
        input: S,
        inverse: bool,
        permit: Option<regolith::sync::OwnedSemaphorePermit>,
    ) -> impl futures::Stream<Item = Result<serde_json::Value>>
    where
        S: futures::Stream + Send + 'static,
        S::Item: serde::Serialize + Send,
    {
        self.get()
            .expect("the test engine builds")
            .call(pipeline, input, inverse, permit)
    }
}
