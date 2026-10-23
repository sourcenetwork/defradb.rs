//! WASM transform store: lens modules compiled and linked on the shared lens
//! engine.
//!
//! Each registered lens keeps its modules' `InstancePre`s in the transform
//! map. A call takes a reference to them, so removing a lens while a call is
//! in flight drops them once that call ends and the map's removed entry is
//! reclaimed (the lock-free map frees removed entries in batches). Resource limits (memory, CPU
//! fuel, epoch interruption, concurrent calls) are opt-in via
//! `WasmSandboxConfig`; the engine, its ticks, its calls and the limits live
//! in the `engine` module. A store builds its engine when it first needs one,
//! so a database that registers no lens builds none (`shared_engine`).

use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt};
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashMap;
use std::path::Path;
use std::sync::Arc;
use wasmtime::Module;

use tracing::{debug, info};

use crate::engine::{CompiledModule, Pipeline, ThreadTicks, TickSource, WasmSandboxConfig};
use crate::store::{first_output, LensDocResultStream, LensDocStream, TransformId, TransformStore};
use crate::{Error, LensConfig, LensDoc, LensModule, Result};
use shared_engine::SharedEngine;

mod shared_engine;

/// WASM-based transform store.
///
/// Manages compiled WASM modules and executes transforms.
pub struct WasmTransformStore {
    engine: SharedEngine,
    modules: HopscotchMap<TransformId, Pipeline, RandomState>,
    configs: HopscotchMap<TransformId, LensConfig, RandomState>,
}

impl WasmTransformStore {
    /// Create a new WASM transform store.
    ///
    /// By default, no resource limits are applied. Pass a `WasmSandboxConfig`
    /// via `with_sandbox` to enable opt-in resource restrictions.
    pub fn new() -> Result<Self> {
        Self::with_sandbox(None)
    }

    /// Create a WASM transform store with optional sandboxing. A thread of its
    /// own advances the epoch when the sandbox sets a deadline.
    pub fn with_sandbox(sandbox: Option<WasmSandboxConfig>) -> Result<Self> {
        Self::with_tick_source(sandbox, ThreadTicks)
    }

    /// Create a WASM transform store whose epoch `ticks` advances. The
    /// sandbox is checked now; the engine is built by the first lens the
    /// store registers.
    pub fn with_tick_source<T>(sandbox: Option<WasmSandboxConfig>, ticks: T) -> Result<Self>
    where
        T: TickSource + 'static,
    {
        Ok(Self {
            engine: SharedEngine::new(sandbox, ticks)?,
            modules: HopscotchMap::with_hasher(RandomState::default()),
            configs: HopscotchMap::with_hasher(RandomState::default()),
        })
    }

    fn empty_with_shared_engine(&self) -> Self {
        Self {
            engine: self.engine.clone(),
            modules: HopscotchMap::with_hasher(RandomState::default()),
            configs: HopscotchMap::with_hasher(RandomState::default()),
        }
    }

    /// Load a WASM module from the given lens configuration.
    ///
    /// When loading from a file path, validates the path to prevent traversal
    /// attacks. The path must be absolute, must not contain `..` segments,
    /// and must have a `.wasm` extension.
    fn load_module(&self, lens: &LensModule) -> Result<Module> {
        if let Some(ref path_str) = lens.path {
            let clean_path = path_str.strip_prefix("file://").unwrap_or(path_str);
            Self::validate_wasm_path(clean_path)?;
            let path = Path::new(clean_path);
            Module::from_file(self.engine.get()?.engine(), path).map_err(|e| {
                Error::WasmLoad(format!(
                    "failed to load WASM from {}: {}",
                    path.display(),
                    e
                ))
            })
        } else if let Some(ref bytes) = lens.module {
            Module::new(self.engine.get()?.engine(), bytes)
                .map_err(|e| Error::WasmLoad(format!("failed to load WASM from bytes: {}", e)))
        } else {
            Err(Error::InvalidConfig(
                "lens module must have either path or module bytes".to_string(),
            ))
        }
    }

    /// Compiles and links every module of a lens, once, at registration.
    fn compile_modules(&self, lenses: &[LensModule]) -> Result<Pipeline> {
        lenses
            .iter()
            .map(|lens| {
                let pre = self.engine.get()?.prepare(&self.load_module(lens)?)?;
                Ok(CompiledModule::new(pre, params(lens)?, lens.inverse))
            })
            .collect::<Result<Vec<_>>>()
            .map(Pipeline::from)
    }

    /// The compiled modules of lens `id`, held for one call.
    fn pipeline(&self, id: &TransformId) -> Result<Pipeline> {
        self.modules.get(id).ok_or_else(|| {
            debug!(transform_id = %id, "Transform not found in WASM store");
            Error::TransformNotFound(id.to_string())
        })
    }

    /// Transform JSON values using a registered lens pipeline.
    ///
    /// This mirrors the standalone host contract where top-level inputs and
    /// outputs are JSON arrays, including `null` items.
    pub async fn transform_json(
        &self,
        id: &TransformId,
        values: Vec<serde_json::Value>,
    ) -> Result<Vec<serde_json::Value>> {
        self.run_to_end(id, values, false).await
    }

    /// Inverse transform JSON values using a registered lens pipeline.
    pub async fn inverse_json(
        &self,
        id: &TransformId,
        values: Vec<serde_json::Value>,
    ) -> Result<Vec<serde_json::Value>> {
        self.run_to_end(id, values, true).await
    }

    /// Runs lens `id` over `values` to the end of its output, under a permit.
    async fn run_to_end(
        &self,
        id: &TransformId,
        values: Vec<serde_json::Value>,
        inverse: bool,
    ) -> Result<Vec<serde_json::Value>> {
        let pipeline = self.pipeline(id)?;
        let engine = self.engine.get()?;
        let permit = engine.permit(pipeline.len()).await;
        engine
            .call(pipeline, futures::stream::iter(values), inverse, permit)
            .try_collect()
            .await
    }

    /// A streaming call of lens `id` over `docs`: it takes no permit, since
    /// its caller may wait on other lens calls while it holds this one.
    fn stream(
        &self,
        id: &TransformId,
        docs: LensDocStream,
        inverse: bool,
    ) -> Result<LensDocResultStream> {
        let pipeline = self.pipeline(id)?;
        debug!(transform_id = %id, modules = pipeline.len(), inverse, "Starting a streaming lens call");
        Ok(Box::pin(
            self.engine
                .get()?
                .call(pipeline, docs, inverse, None)
                .map(|value| value.and_then(into_doc)),
        ))
    }

    /// Validate a WASM module file path to prevent path traversal.
    fn validate_wasm_path(path_str: &str) -> Result<()> {
        let path = Path::new(path_str);

        if !path.is_absolute() {
            return Err(Error::PathNotAllowed(
                "WASM module path must be absolute".to_string(),
            ));
        }

        for component in path.components() {
            if let std::path::Component::ParentDir = component {
                return Err(Error::PathNotAllowed(
                    "WASM module path must not contain '..' segments".to_string(),
                ));
            }
        }

        match path.extension().and_then(|e| e.to_str()) {
            Some("wasm") => {}
            _ => {
                return Err(Error::PathNotAllowed(
                    "WASM module path must have .wasm extension".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// The JSON a module's `set_param` receives: none when the lens has no
/// arguments or an empty object.
fn params(lens: &LensModule) -> Result<Option<Vec<u8>>> {
    match &lens.arguments {
        None => Ok(None),
        Some(serde_json::Value::Object(map)) if map.is_empty() => Ok(None),
        Some(arguments) => serde_json::to_vec(arguments)
            .map(Some)
            .map_err(|e| Error::WasmExecution(format!("failed to serialize params: {}", e))),
    }
}

/// A document a lens yields: it must be a JSON object.
fn into_doc(value: serde_json::Value) -> Result<LensDoc> {
    match value {
        serde_json::Value::Object(doc) => Ok(doc),
        other => Err(Error::WasmExecution(format!(
            "expected JSON object output, got {}",
            json_value_type(&other)
        ))),
    }
}

#[async_trait]
impl TransformStore for WasmTransformStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        Arc::new(self.empty_with_shared_engine())
    }

    async fn add(&self, config: LensConfig) -> Result<TransformId> {
        use sha2::{Digest, Sha256};

        let lenses_json = serde_json::to_vec(&config.lenses)
            .map_err(|e| Error::Pipeline(format!("failed to serialize lenses: {}", e)))?;
        let mut hasher = Sha256::new();
        hasher.update(&lenses_json);
        let hash = hasher.finalize();
        let id = TransformId::new(format!("baf{}", hex::encode(&hash[..16])));

        if self.modules.contains_key(&id) {
            debug!(transform_id = %id, "Transform already exists, returning existing ID");
            return Ok(id);
        }

        let compiled_modules = self.compile_modules(&config.lenses)?;
        info!(
            transform_id = %id,
            source_version = %config.source_collection_version_id,
            dest_version = %config.destination_collection_version_id,
            lenses_count = config.lenses.len(),
            "Transform added to WASM store"
        );
        self.modules.insert_if_absent(id.clone(), compiled_modules);
        self.configs.insert_if_absent(id.clone(), config);

        Ok(id)
    }

    async fn add_with_id(&self, id: TransformId, config: LensConfig) -> Result<()> {
        if self.modules.contains_key(&id) {
            debug!(transform_id = %id, "Transform already exists");
            return Ok(());
        }

        let compiled_modules = self.compile_modules(&config.lenses)?;
        self.modules.insert_if_absent(id.clone(), compiled_modules);
        self.configs.insert_if_absent(id.clone(), config);

        info!(transform_id = %id, "Transform added to WASM store with explicit ID");
        Ok(())
    }

    async fn list(&self) -> Result<RapidHashMap<String, crate::LensModule>> {
        let result = self
            .configs
            .iter()
            .filter_map(|(id, config)| config.lens().cloned().map(|l| (id.to_string(), l)))
            .collect();
        Ok(result)
    }

    fn transform(&self, id: &TransformId, docs: LensDocStream) -> Result<LensDocResultStream> {
        self.stream(id, docs, false)
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> Result<LensDocResultStream> {
        self.stream(id, docs, true)
    }

    /// One document, run to its first output under a permit.
    async fn transform_doc(
        &self,
        id: &TransformId,
        doc: LensDoc,
        inverse: bool,
    ) -> Result<LensDoc> {
        let pipeline = self.pipeline(id)?;
        let engine = self.engine.get()?;
        let permit = engine.permit(pipeline.len()).await;
        let output = engine
            .call(pipeline, futures::stream::iter([doc]), inverse, permit)
            .map(|value| value.and_then(into_doc));
        first_output(output).await
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.modules.contains_key(id)
    }

    async fn remove(&self, id: &TransformId) -> Result<()> {
        if self.modules.remove(id).is_none() {
            return Err(Error::TransformNotFound(id.to_string()));
        }
        self.configs.remove(id);
        Ok(())
    }
}

fn json_value_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests;
