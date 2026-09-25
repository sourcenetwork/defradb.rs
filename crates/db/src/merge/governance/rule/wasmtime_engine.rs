//! The rule engine on wasmtime: compiled, native only.

use wasmtime::{
    Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder, WasmFeatures,
};

use super::engine::response_span;
use super::RuleBudget;

pub(crate) struct Runtime {
    engine: Engine,
}

impl Runtime {
    pub(crate) fn new() -> Result<Self, String> {
        let mut config = Config::new();
        config.consume_fuel(true);
        // The profile wasmi runs (see `engine`): wasmtime's defaults add
        // SIMD, relaxed SIMD and memory64, which wasmi refuses, and a module
        // this engine alone compiles would be a rule only some nodes can run.
        config.wasm_features(WasmFeatures::SIMD, false);
        config.wasm_features(WasmFeatures::RELAXED_SIMD, false);
        config.wasm_features(WasmFeatures::MEMORY64, false);
        config.wasm_features(WasmFeatures::MULTI_MEMORY, true);
        config.wasm_features(WasmFeatures::TAIL_CALL, true);
        config.wasm_features(WasmFeatures::EXTENDED_CONST, true);
        config.cranelift_nan_canonicalization(true);
        #[cfg(target_os = "macos")]
        {
            // Wasmtime installs trap handling once per process and panics if a
            // second engine asks for a different kind. `lens` embeds wasmtime
            // too and turns Mach ports off (fork-capable embedders crash when
            // the Mach-port exception handler is initialised before spawning),
            // and both engines live in one process, so this engine has to
            // agree with it.
            config.macos_use_mach_ports(false);
        }
        let engine = Engine::new(&config).map_err(|error| error.to_string())?;
        Ok(Self { engine })
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
            .get_memory(&mut store, "memory")
            .ok_or("rule module exports no memory")?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|error| format!("rule module exports no alloc: {error}"))?;
        let judge = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "judge")
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
