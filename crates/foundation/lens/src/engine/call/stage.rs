//! One module of a call: its store, its instance, and its loop of
//! `transform` (or `inverse`) calls until it ends its stream.

use serde::de::IgnoredAny;
use wasmtime::{AsContextMut, Instance, Store};

use super::channel::{ByteReceiver, ByteSender};
use super::guest::{Guest, Output};
use super::{failure, resume, store, CallData, Pipeline};
use crate::engine::LensEngine;
use crate::{Error, Result};

/// Where a module sits in its call.
pub(super) struct Stage {
    pub(super) pipeline: Pipeline,
    pub(super) index: usize,
    /// The call runs the pipeline backwards.
    pub(super) inverse: bool,
    /// No module reads this one's output, so the caller parses it.
    pub(super) last: bool,
}

/// Runs module `stage.index` over `input` until it ends its stream, sending
/// each document it yields to `output`.
pub(super) async fn run(
    engine: LensEngine,
    stage: Stage,
    input: ByteReceiver,
    output: ByteSender,
) -> Result<()> {
    let module = &stage.pipeline[stage.index];
    let mut store = store(&engine, input)?;
    let instance = module
        .pre
        .instantiate_async(&mut store)
        .await
        .map_err(|e| failure("failed to instantiate", e))?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| Error::WasmExecution("no memory export".to_string()))?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut store, "alloc")
        .ok();
    let guest = Guest::new(memory, alloc);
    store.data_mut().guest = Some(guest.clone());

    if let Some(params) = &module.params {
        set_param(&mut store, &instance, &guest, params).await?;
    }

    let func_name = if module.inverse != stage.inverse {
        "inverse"
    } else {
        "transform"
    };
    let transform = instance
        .get_typed_func::<(), i32>(&mut store, func_name)
        .map_err(|e| {
            if func_name == "inverse" && e.to_string().contains("failed to find function export") {
                Error::WasmExecution("Export `inverse` does not exist".to_string())
            } else {
                Error::WasmExecution(format!("{func_name} func not found: {e}"))
            }
        })?;

    loop {
        resume(store.as_context_mut());
        let offset = transform
            .call_async(&mut store, ())
            .await
            .map_err(|e| failure(&format!("{func_name} call failed"), e))?;
        if offset == 0 {
            return Ok(());
        }
        match guest
            .read(&store, offset)
            .map_err(|e| failure("reading the module's output failed", e))?
        {
            Output::Eos => return Ok(()),
            Output::Error(message) => {
                return Err(Error::WasmExecution(format!("WASM error: {message}")));
            }
            Output::Nil => output.send(b"null".to_vec()).await,
            Output::Json(json) => {
                if !stage.last {
                    serde_json::from_slice::<IgnoredAny>(&json)
                        .map_err(|e| Error::WasmExecution(e.to_string()))?;
                }
                output.send(json).await;
            }
        }
    }
}

/// Hands the module its parameters through `set_param`.
async fn set_param(
    store: &mut Store<CallData>,
    instance: &Instance,
    guest: &Guest,
    params: &[u8],
) -> Result<()> {
    let set_param = instance
        .get_typed_func::<i32, i32>(&mut *store, "set_param")
        .map_err(|_| Error::WasmExecution("Export `set_param` does not exist".to_string()))?;
    let offset = guest
        .write(&mut *store, Some(params))
        .await
        .map_err(|e| failure("writing the parameters failed", e))?;
    let result = set_param
        .call_async(&mut *store, offset)
        .await
        .map_err(|e| failure("set_param failed", e))?;
    match guest
        .read(&*store, result)
        .map_err(|e| failure("reading set_param's result failed", e))?
    {
        Output::Error(message) => Err(Error::WasmExecution(format!("WASM error: {message}"))),
        Output::Nil | Output::Json(_) | Output::Eos => Ok(()),
    }
}
