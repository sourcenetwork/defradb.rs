//! Which wasm engine runs a rule module, behind one guest ABI.
//!
//! A verdict is replicated behaviour, so the engine must not be an input to
//! it: the same module over the same request answers the same bytes on
//! either engine, or fails on either. What differs is cost. Each engine
//! meters fuel in its own units, so the same [`RuleBudget`] admits more
//! instructions on one than on the other, and a module close to its budget
//! can finish on one engine and run out on the other. Running out is an
//! error, never a verdict: the composite stays unmerged and the sweep tries
//! again. So an engine choice can cost liveness on a node, never agreement,
//! and a rule that needs more than a small fraction of its budget is a rule
//! to fix. Nodes that must converge on the same schedule, such as a relay
//! and the browsers it serves, should run the same engine.
//!
//! Both engines are configured to the same wasm feature profile: the 2.0
//! core without SIMD, plus multi-memory, tail calls and extended constants,
//! with NaNs canonicalised. A module one engine refuses to compile the
//! other refuses too, and float results carry no engine-chosen bits.

use std::fmt;
use std::str::FromStr;

use super::RuleBudget;

/// The engine a [`super::WasmRules`] runs its modules on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleEngine {
    /// A pure-Rust interpreter. Builds for every target, the browser
    /// included.
    Wasmi,
    /// A compiling engine. Native targets only.
    Wasmtime,
}

impl RuleEngine {
    /// Whether this build can run the engine.
    pub fn is_available(self) -> bool {
        match self {
            RuleEngine::Wasmi => true,
            RuleEngine::Wasmtime => cfg!(not(target_arch = "wasm32")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RuleEngine::Wasmi => "wasmi",
            RuleEngine::Wasmtime => "wasmtime",
        }
    }
}

/// The compiling engine where there is one: a native node's default is the
/// faster engine, and a browser's is the only one it has.
impl Default for RuleEngine {
    fn default() -> Self {
        if cfg!(target_arch = "wasm32") {
            RuleEngine::Wasmi
        } else {
            RuleEngine::Wasmtime
        }
    }
}

impl fmt::Display for RuleEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RuleEngine {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "wasmi" => Ok(RuleEngine::Wasmi),
            "wasmtime" => Ok(RuleEngine::Wasmtime),
            other => Err(format!(
                "unknown rule engine '{other}': expected wasmi or wasmtime"
            )),
        }
    }
}

/// An engine instance, holding what it compiles modules against.
pub(super) enum Runtime {
    Wasmi(super::wasmi_engine::Runtime),
    #[cfg(not(target_arch = "wasm32"))]
    Wasmtime(super::wasmtime_engine::Runtime),
}

/// A module compiled by one [`Runtime`], run only by that runtime.
pub(super) enum Compiled {
    Wasmi(wasmi::Module),
    #[cfg(not(target_arch = "wasm32"))]
    Wasmtime(wasmtime::Module),
}

impl Runtime {
    pub(super) fn new(engine: RuleEngine) -> Result<Self, String> {
        match engine {
            RuleEngine::Wasmi => Ok(Runtime::Wasmi(super::wasmi_engine::Runtime::new())),
            #[cfg(not(target_arch = "wasm32"))]
            RuleEngine::Wasmtime => super::wasmtime_engine::Runtime::new().map(Runtime::Wasmtime),
            #[cfg(target_arch = "wasm32")]
            RuleEngine::Wasmtime => Err("wasmtime is not available on this target".to_string()),
        }
    }

    pub(super) fn engine(&self) -> RuleEngine {
        match self {
            Runtime::Wasmi(_) => RuleEngine::Wasmi,
            #[cfg(not(target_arch = "wasm32"))]
            Runtime::Wasmtime(_) => RuleEngine::Wasmtime,
        }
    }

    pub(super) fn compile(&self, bytes: &[u8]) -> Result<Compiled, String> {
        match self {
            Runtime::Wasmi(runtime) => runtime.compile(bytes).map(Compiled::Wasmi),
            #[cfg(not(target_arch = "wasm32"))]
            Runtime::Wasmtime(runtime) => runtime.compile(bytes).map(Compiled::Wasmtime),
        }
    }

    /// One call of the module's `judge` over `request`, in a fresh instance,
    /// returning the response bytes without their length prefix.
    pub(super) fn run(
        &self,
        module: &Compiled,
        request: &[u8],
        budget: &RuleBudget,
    ) -> Result<Vec<u8>, String> {
        match (self, module) {
            (Runtime::Wasmi(runtime), Compiled::Wasmi(module)) => {
                runtime.run(module, request, budget)
            }
            #[cfg(not(target_arch = "wasm32"))]
            (Runtime::Wasmtime(runtime), Compiled::Wasmtime(module)) => {
                runtime.run(module, request, budget)
            }
            #[cfg(not(target_arch = "wasm32"))]
            _ => Err("rule module was compiled by another engine".to_string()),
        }
    }
}

/// The response's length prefix at `out`, checked against the memory it
/// must fit in before anything is allocated for it.
pub(super) fn response_span(
    out: i32,
    header: [u8; 4],
    memory_len: usize,
) -> Result<(usize, usize), String> {
    let start = usize::try_from(out)
        .map_err(|_| "rule module response pointer is negative".to_string())?
        .checked_add(4)
        .ok_or("rule module response pointer overflows")?;
    let len = u32::from_le_bytes(header) as usize;
    match start.checked_add(len) {
        Some(end) if end <= memory_len => Ok((start, len)),
        _ => Err(format!(
            "rule module response of {len} bytes runs past its memory"
        )),
    }
}
