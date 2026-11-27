//! One wasmtime engine with its sandbox, its linker, its call permits and its
//! epoch ticks, shared by its clones.

use std::sync::Arc;

use futures::Stream;
use regolith::sync::{OwnedSemaphorePermit, Semaphore};
use serde::Serialize;
use wasmtime::{Config, Engine, InstancePre, Linker, Module};

use super::call::{self, CallData, CallStream, Pipeline};
use super::clock::{EpochTick, TickSource, Ticking};
use super::sandbox::WasmSandboxConfig;
use crate::{Error, Result};

/// A wasmtime engine, the sandbox its calls run under, the one linker every
/// module links against, the permits that bound calls run to completion, and
/// the ticks that advance its epoch. Clones share all of them; the ticks stop
/// when the last clone is dropped.
///
/// The engine is built with fuel and epoch interruption as the sandbox asks,
/// and with wasmtime's `async` support, so a call runs on a fiber its caller
/// polls. wasmtime is built without parallel compilation, so it starts no
/// thread of its own.
#[derive(Clone)]
pub(crate) struct LensEngine {
    engine: Engine,
    sandbox: Option<WasmSandboxConfig>,
    linker: Arc<Linker<CallData>>,
    calls: Option<Calls>,
    /// The ticks run while any clone holds them.
    ticks: Option<Arc<dyn Ticking>>,
}

/// The permits of calls run to completion, and how many there are.
#[derive(Clone)]
struct Calls {
    permits: Arc<Semaphore>,
    max: usize,
}

/// The settings every wasmtime engine of the process is built with: wasmtime
/// sets up its trap handlers once per process, from the first engine, and
/// every later engine must agree with it.
pub(super) fn base_config() -> Config {
    // Only macOS has a setting to change.
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut config = Config::new();
    #[cfg(target_os = "macos")]
    {
        // Fork-capable embedders can crash when Wasmtime's Mach-port
        // exception handler is initialized before spawning child processes.
        config.macos_use_mach_ports(false);
    }
    config
}

impl LensEngine {
    /// Build an engine whose epoch `ticks` advances. Without a sandbox, calls
    /// run without limits and nothing ticks.
    pub(crate) fn new(sandbox: Option<WasmSandboxConfig>, ticks: &dyn TickSource) -> Result<Self> {
        let mut config = base_config();
        if let Some(ref sb) = sandbox {
            sb.validate()?;
            if sb.fuel_budget.is_some() {
                config.consume_fuel(true);
            }
            if sb.epoch_deadline_ticks.is_some() {
                config.epoch_interruption(true);
            }
        }
        let engine = Engine::new(&config)
            .map_err(|e| Error::WasmLoad(format!("failed to create WASM engine: {}", e)))?;
        let ticks = match sandbox.as_ref().and_then(WasmSandboxConfig::tick_period) {
            Some(period) => Some(Arc::from(ticks.start(EpochTick(engine.clone()), period)?)),
            None => None,
        };
        let calls = sandbox
            .as_ref()
            .and_then(|sb| sb.max_concurrent_calls)
            .map(|max| Calls {
                permits: Arc::new(Semaphore::new(max)),
                max,
            });
        let linker = Arc::new(call::linker(&engine)?);

        Ok(Self {
            engine,
            sandbox,
            linker,
            calls,
            ticks,
        })
    }

    /// The wasmtime engine, to compile modules on.
    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The limits every call runs under.
    pub(crate) fn sandbox(&self) -> Option<&WasmSandboxConfig> {
        self.sandbox.as_ref()
    }

    /// Links `module` against the engine's linker, once, when it is
    /// registered: every call instantiates from what this returns.
    pub(crate) fn prepare(&self, module: &Module) -> Result<InstancePre<CallData>> {
        self.linker
            .instantiate_pre(module)
            .map_err(|e| Error::WasmLoad(format!("failed to link WASM module: {}", e)))
    }

    /// Waits for the permits of a call run to completion over a pipeline of
    /// `modules` modules: one per module, taken all at once, so no call holds
    /// some while it waits for the rest. A pipeline longer than the bound
    /// takes every permit and runs alone. `None` when calls are unbounded.
    pub(crate) async fn permit(&self, modules: usize) -> Option<OwnedSemaphorePermit> {
        let calls = self.calls.as_ref()?;
        Some(
            calls
                .permits
                .acquire_owned(modules.clamp(1, calls.max))
                .await,
        )
    }

    /// Starts a call of `pipeline` over `input`, backwards when `inverse`,
    /// holding `permit` until it ends.
    pub(crate) fn call<S>(
        &self,
        pipeline: Pipeline,
        input: S,
        inverse: bool,
        permit: Option<OwnedSemaphorePermit>,
    ) -> CallStream
    where
        S: Stream + Send + 'static,
        S::Item: Serialize + Send,
    {
        call::start(self, pipeline, input, inverse, permit)
    }

    /// Starts the ticks a call needs, where they start on demand.
    pub(crate) fn ticking(&self) -> Result<()> {
        self.ticks.as_ref().map_or(Ok(()), |ticks| ticks.calling())
    }

    /// A handle that shows whether the ticks still run, for tests of their
    /// lifetime.
    #[cfg(test)]
    pub(crate) fn ticks_handle(&self) -> Option<std::sync::Weak<dyn Ticking>> {
        self.ticks.as_ref().map(Arc::downgrade)
    }
}
