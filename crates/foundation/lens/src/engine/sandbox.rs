//! The limits a lens call runs under.

use std::num::NonZeroUsize;
use std::time::Duration;

use regolith::sync::{Lazy, Semaphore};

use crate::{Error, Result};

/// The cores of this process, read once: every database opened reads them.
static CORES: Lazy<usize> =
    Lazy::new(|| std::thread::available_parallelism().map_or(1, NonZeroUsize::get));

/// Opt-in resource limits for WASM execution.
///
/// All fields are optional. When `None`, the corresponding limit is not applied,
/// giving the WASM module full access to that resource.
///
/// Fuel and ticks are budgets per input document: each document a call takes
/// in through `lens::next` refills both. A stream of any length runs, while a
/// call that keeps running without taking a document (a loop that never
/// ends, output that never ends) traps.
#[derive(Debug, Clone)]
pub struct WasmSandboxConfig {
    /// Maximum memory a single WASM instance may allocate (bytes).
    pub max_memory_bytes: Option<usize>,

    /// Fuel per input document (roughly an instruction count).
    pub fuel_budget: Option<u64>,

    /// Epoch ticks a call may run per input document before it traps. A call
    /// yields to the executor polling it on every tick it runs through.
    pub epoch_deadline_ticks: Option<u64>,

    /// Period at which the shared engine advances its epoch clock. The 100 ms
    /// restrictive default is a measured provisional setting; tune it for the
    /// workload with the ignored probe described in
    /// `docs/benchmarks/lens-epoch-period/measurement.md`.
    pub epoch_tick_period: Option<Duration>,

    /// The most lens modules that run to completion for their caller (one
    /// document's migration, a JSON batch) at once; a call takes one permit
    /// per module of its lens and waits while they are not free. Each module
    /// holds at most `max_memory_bytes` of instance memory and the fiber
    /// stacks it runs on, so this bounds their memory.
    ///
    /// A streaming call (a lensed view reading its source) takes no permit: it
    /// waits on its source, which may itself run lens calls, so a permit held
    /// across that wait could leave every permit held by calls waiting for
    /// one. Its memory is bounded by the reads in flight instead.
    pub max_concurrent_calls: Option<usize>,
}

impl WasmSandboxConfig {
    /// Recommended defaults for high-security environments. Calls that run to
    /// completion are bounded at one per core: they only compute, so more
    /// would add memory without adding throughput.
    pub fn restrictive() -> Self {
        Self {
            max_memory_bytes: Some(64 * 1024 * 1024), // 64 MiB
            fuel_budget: Some(1_000_000),
            epoch_deadline_ticks: Some(2),
            epoch_tick_period: Some(Duration::from_millis(100)),
            max_concurrent_calls: Some(*CORES),
        }
    }

    /// Refuses limits an engine cannot run under: an epoch deadline without
    /// a positive tick period, or a call bound outside the permits a
    /// semaphore holds. A store checks them when it is made, before it
    /// builds the engine they configure.
    pub(crate) fn validate(&self) -> Result<()> {
        match (self.epoch_deadline_ticks, self.epoch_tick_period) {
            (Some(_), Some(period)) if period.is_zero() => {
                return Err(Error::InvalidConfig(
                    "WASM epoch tick period must be greater than zero".into(),
                ));
            }
            (Some(_), None) => {
                return Err(Error::InvalidConfig(
                    "WASM epoch interruption requires an epoch tick period".into(),
                ));
            }
            _ => {}
        }
        match self.max_concurrent_calls {
            Some(max) if max == 0 || max > Semaphore::MAX_PERMITS => {
                Err(Error::InvalidConfig(format!(
                    "max_concurrent_calls must be between 1 and {}",
                    Semaphore::MAX_PERMITS
                )))
            }
            _ => Ok(()),
        }
    }

    /// The period the engine's epoch ticks at, when calls have a deadline.
    pub(crate) fn tick_period(&self) -> Option<Duration> {
        self.epoch_deadline_ticks.and(self.epoch_tick_period)
    }
}
