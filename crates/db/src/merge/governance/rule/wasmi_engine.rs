//! The rule engine on wasmi: interpreted, pure Rust, every target.

use wasmi::{Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder};

use super::engine::response_span;
use super::RuleBudget;

pub(crate) struct Runtime {
    engine: Engine,
}

impl Runtime {
    pub(crate) fn new() -> Self {
        let mut config = Config::default();
        config.consume_fuel(true);
        // wasmi's defaults are the shared profile already (see `engine`):
        // SIMD and memory64 are compiled out of this build, and NaNs are
        // canonicalised by its `deterministic` feature. Stated here so a
        // change of wasmi's defaults cannot move the profile silently.
        config.wasm_multi_memory(true);
        config.wasm_tail_call(true);
        config.wasm_extended_const(true);
        config.wasm_custom_page_sizes(false);
        config.wasm_wide_arithmetic(false);
        Self {
            engine: Engine::new(&config),
        }
    }

    pub(crate) fn compile(&self, bytes: &[u8]) -> Result<Module, String> {
        Module::new(&self.engine, bytes).map_err(|error| error.to_string())
    }

    pub(crate) fn run(
        &self,
        module: &Module,
        request: &[u8],
        budget: &RuleBudget,
    ) -> Result<Vec<u8>, String> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(budget.memory_bytes)
            .build();
        let mut store: Store<StoreLimits> = Store::new(&self.engine, limits);
        store.limiter(|limits| limits);
        store
            .set_fuel(budget.fuel)
            .map_err(|error| error.to_string())?;
        let instance = Instance::new(&mut store, module, &[])
            .map_err(|error| format!("rule module does not instantiate: {error}"))?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or("rule module exports no memory")?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&store, "alloc")
            .map_err(|error| format!("rule module exports no alloc: {error}"))?;
        let judge = instance
            .get_typed_func::<(i32, i32), i32>(&store, "judge")
            .map_err(|error| format!("rule module exports no judge: {error}"))?;

        let len = i32::try_from(request.len()).map_err(|_| "request too large")?;
        let ptr = alloc
            .call(&mut store, len)
            .map_err(|error| format!("rule module alloc failed: {error}"))?;
        let ptr =
            usize::try_from(ptr).map_err(|_| "rule module alloc returned a negative pointer")?;
        memory
            .write(&mut store, ptr, request)
            .map_err(|error| format!("rule module memory write failed: {error}"))?;
        let out = judge
            .call(&mut store, (ptr as i32, len))
            .map_err(|error| format!("rule module trapped: {error}"))?;
        let mut header = [0u8; 4];
        memory
            .read(
                &store,
                usize::try_from(out).unwrap_or(usize::MAX),
                &mut header,
            )
            .map_err(|error| format!("rule module response unreadable: {error}"))?;
        let (start, out_len) = response_span(out, header, memory.data_size(&store))?;
        let mut response = vec![0u8; out_len];
        memory
            .read(&store, start, &mut response)
            .map_err(|error| format!("rule module response unreadable: {error}"))?;
        Ok(response)
    }
}
