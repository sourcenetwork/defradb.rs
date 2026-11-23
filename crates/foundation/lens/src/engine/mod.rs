//! The lens engine: the one WASM runtime a database runs its lens transforms
//! on, and the store a database gets when no runtime is built in.
//!
//! A database takes its store from [`default_store`]. With the
//! `wasmtime-runtime` feature that store runs on one `LensEngine`: a wasmtime
//! engine, the sandbox its calls run under, the one `Linker` every module
//! links against, and what advances its epoch (a [`TickSource`]). Every store
//! derived from it, a transaction's registrations included, shares all of
//! them: a transaction never builds an engine, a linker or a clock of its
//! own. The ticks stop with the last clone of the engine, and a clock thread
//! that panicked is reported then.
//!
//! A lens module is compiled and linked once, when it is registered, into an
//! `InstancePre`. Each call (the `call` module) instantiates it in a store of
//! its own, made here so the limits a call runs under always match the
//! features the engine was built with, and runs it on a fiber its caller
//! polls: the call streams its documents, yields on every epoch tick, and
//! traps past the sandbox's fuel or ticks for one input document.
//!
//! Without the `wasmtime-runtime` feature there is no engine: the default store
//! is [`UnsupportedTransformStore`], which refuses registration and execution
//! with an error that names the missing feature.

#[cfg(feature = "wasmtime-runtime")]
mod call;
#[cfg(feature = "wasmtime-runtime")]
mod clock;
#[cfg(feature = "wasmtime-runtime")]
mod sandbox;
#[cfg(feature = "wasmtime-runtime")]
mod shared;
#[cfg(all(test, feature = "wasmtime-runtime"))]
pub(crate) mod tests;
mod unsupported;

use std::sync::Arc;

#[cfg(feature = "wasmtime-runtime")]
pub(crate) use call::{CompiledModule, Pipeline};
#[cfg(feature = "wasmtime-runtime")]
pub(crate) use clock::ThreadTicks;
#[cfg(feature = "wasmtime-runtime")]
pub use clock::{EpochTick, PoolTicks, TickGuard, TickSource, Ticking};
#[cfg(feature = "wasmtime-runtime")]
pub use sandbox::WasmSandboxConfig;
#[cfg(feature = "wasmtime-runtime")]
pub(crate) use shared::LensEngine;
pub use unsupported::UnsupportedTransformStore;

use crate::{Result, TransformStore};

/// The transform store a database runs its lens migrations on: WASM on one
/// shared engine under [`WasmSandboxConfig::restrictive`].
#[cfg(feature = "wasmtime-runtime")]
pub fn default_store() -> Result<Arc<dyn TransformStore>> {
    let store = crate::WasmTransformStore::with_sandbox(Some(WasmSandboxConfig::restrictive()))?;
    Ok(Arc::new(store))
}

/// [`default_store`] whose epoch ticks are a job of `pool`'s deadline queue.
#[cfg(feature = "wasmtime-runtime")]
pub fn default_store_on(pool: &workers::Pool) -> Result<Arc<dyn TransformStore>> {
    let store = crate::WasmTransformStore::with_tick_source(
        Some(WasmSandboxConfig::restrictive()),
        PoolTicks(pool.clone()),
    )?;
    Ok(Arc::new(store))
}

/// The transform store a database runs its lens migrations on. Built without
/// the `wasmtime-runtime` feature, it refuses every transform.
#[cfg(not(feature = "wasmtime-runtime"))]
pub fn default_store() -> Result<Arc<dyn TransformStore>> {
    Ok(Arc::new(UnsupportedTransformStore))
}

/// [`default_store`]: a build without the runtime ticks nothing.
#[cfg(not(feature = "wasmtime-runtime"))]
pub fn default_store_on(_pool: &workers::Pool) -> Result<Arc<dyn TransformStore>> {
    default_store()
}
