//! One lens call: a pipeline of compiled modules run over a stream of
//! documents.
//!
//! Each module of the pipeline runs in a wasmtime store of its own, on a fiber
//! the call's caller polls ([`CallStream`]): `instantiate_async` from the
//! module's `InstancePre`, then `call_async` of `transform` (or `inverse`)
//! until the module ends its stream. Documents flow between the caller and
//! the modules through channels bounded by bytes, so no side holds the whole
//! input, and a module may combine documents across the whole stream.
//!
//! What must never happen: a call that runs on unbounded (each store has the
//! sandbox's fuel and ticks per input document, and traps past them), a call
//! that keeps its worker from other tasks (it yields on every epoch tick), or
//! a module dropped under a call that runs it (the pipeline is
//! reference-counted, so a removed lens lives until the calls on it end).

mod channel;
mod guest;
mod host;
mod stage;
mod stream;

use std::sync::Arc;

use wasmtime::{
    AsContextMut, InstancePre, Store, StoreContextMut, StoreHookState, StoreLimits,
    StoreLimitsBuilder, Trap, UpdateDeadline,
};

use super::LensEngine;
use crate::{Error, Result};
use channel::ByteReceiver;
use guest::Guest;

pub(in crate::engine) use host::linker;
#[cfg(test)]
pub(in crate::engine) use host::linkers_built;
pub(in crate::engine) use stream::start;
pub(crate) use stream::CallStream;

/// A lens module compiled on the engine and linked against its one linker,
/// ready to instantiate for a call.
pub(crate) struct CompiledModule {
    pre: InstancePre<CallData>,
    params: Option<Vec<u8>>,
    inverse: bool,
}

impl CompiledModule {
    /// `params` is the JSON handed to the module's `set_param` before its
    /// first document; `inverse` runs the module backwards.
    pub(crate) fn new(pre: InstancePre<CallData>, params: Option<Vec<u8>>, inverse: bool) -> Self {
        Self {
            pre,
            params,
            inverse,
        }
    }
}

/// The modules of one lens, in order. Every call on the lens holds it, so a
/// removed lens lives until its last call ends.
pub(crate) type Pipeline = Arc<[CompiledModule]>;

/// What one module's store holds: the call's input, the module's exports once
/// it is instantiated, and the limits it runs under.
pub(crate) struct CallData {
    input: ByteReceiver,
    guest: Option<Guest>,
    input_ended: bool,
    budget: Budget,
    ticks_left: u64,
    limits: StoreLimits,
}

/// The sandbox's fuel and ticks, granted per input document.
#[derive(Clone, Copy)]
struct Budget {
    fuel: Option<u64>,
    ticks: Option<u64>,
}

/// A store for one module of a call, under the engine's sandbox: its memory
/// limit, and fuel and ticks for its first document. The engine turns fuel
/// and epoch interruption on exactly when the sandbox sets them, so the two
/// always agree.
fn store(engine: &LensEngine, input: ByteReceiver) -> Result<Store<CallData>> {
    let sandbox = engine.sandbox();
    let limits = sandbox.and_then(|sb| sb.max_memory_bytes).map_or_else(
        || StoreLimitsBuilder::new().build(),
        |max| StoreLimitsBuilder::new().memory_size(max).build(),
    );
    let budget = Budget {
        fuel: sandbox.and_then(|sb| sb.fuel_budget),
        ticks: sandbox.and_then(|sb| sb.epoch_deadline_ticks),
    };
    let data = CallData {
        input,
        guest: None,
        input_ended: false,
        budget,
        ticks_left: 0,
        limits,
    };
    let mut store = Store::new(engine.engine(), data);
    store.limiter(|data| &mut data.limits);
    if budget.ticks.is_some() {
        store.epoch_deadline_callback(on_tick);
    }
    refill(store.as_context_mut()).map_err(|e| failure("failed to set the call's budget", e))?;
    Ok(store)
}

/// Grants a fresh budget: the sandbox's fuel, its ticks, and the deadline of
/// a slice that starts now ([`SLICE`]).
fn refill(mut store: StoreContextMut<'_, CallData>) -> wasmtime::Result<()> {
    let budget = store.data().budget;
    if let Some(fuel) = budget.fuel {
        store.set_fuel(fuel)?;
    }
    if let Some(ticks) = budget.ticks {
        store.data_mut().ticks_left = ticks;
        store.set_epoch_deadline(SLICE);
    }
    Ok(())
}

/// The deadline of a slice that starts between ticks, in ticks from now: the
/// first tick can land at once, so the slice ends at the second and runs at
/// least one full tick period. A call shorter than a period never yields.
const SLICE: u64 = 2;

/// Before the module runs again after a wait: the ticks that passed while it
/// waited were not its own, so a new slice starts ([`SLICE`]).
fn resume(mut store: StoreContextMut<'_, CallData>) {
    if store.data().budget.ticks.is_some() {
        store.set_epoch_deadline(SLICE);
    }
}

/// Each epoch tick a module runs through: it yields to the executor polling
/// the call, and traps once its ticks are spent.
fn on_tick(mut store: StoreHookState<'_, CallData>) -> wasmtime::Result<UpdateDeadline> {
    let data = store.data_mut();
    data.ticks_left = data.ticks_left.saturating_sub(1);
    Ok(if data.ticks_left == 0 {
        UpdateDeadline::Interrupt
    } else {
        UpdateDeadline::Yield(1)
    })
}

/// A wasmtime failure as a lens error: a call past its ticks or its fuel gets
/// an error of its own, anything else names what failed.
fn failure(context: &str, error: wasmtime::Error) -> Error {
    match error.downcast_ref::<Trap>() {
        Some(Trap::Interrupt) => Error::TimeLimit,
        Some(Trap::OutOfFuel) => Error::FuelLimit,
        _ => Error::WasmExecution(format!("{context}: {error}")),
    }
}
