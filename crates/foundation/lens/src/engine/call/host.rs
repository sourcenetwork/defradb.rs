//! The host functions a lens module imports, defined once per engine.

use wasmtime::{AsContextMut, Caller, Engine, Linker};

use super::{refill, CallData};
use crate::{Error, Result};

#[cfg(test)]
thread_local! {
    static LINKERS_BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The linkers built on this thread, for the test that a call builds none.
#[cfg(test)]
pub(in crate::engine) fn linkers_built() -> usize {
    LINKERS_BUILT.with(std::cell::Cell::get)
}

/// The engine's one linker. `lens::next` finds everything it needs in its
/// store's data (the call's input channel and the module's exports), so the
/// linker captures nothing of any call and serves every call on the engine.
pub(in crate::engine) fn linker(engine: &Engine) -> Result<Linker<CallData>> {
    #[cfg(test)]
    LINKERS_BUILT.with(|built| built.set(built.get() + 1));
    let mut linker = Linker::new(engine);
    linker
        .func_wrap_async(
            "lens",
            "next",
            |mut caller: Caller<'_, CallData>, (): ()| {
                Box::new(async move { next(&mut caller).await })
            },
        )
        .map_err(|e| Error::WasmLoad(format!("failed to define lens::next: {e}")))?;
    Ok(linker)
}

/// `lens::next`: the call's next input document, or the end of the stream
/// once the input is closed. The call's fiber suspends while the input is
/// empty. Taking a document in refills the call's fuel and ticks, and so does
/// the first end of the stream, for the work a module does after its input
/// (an aggregate); asking again after the end refills nothing.
async fn next(caller: &mut Caller<'_, CallData>) -> wasmtime::Result<i32> {
    let input = caller.data().input.clone();
    let document = input.recv().await;
    let guest =
        caller.data().guest.clone().ok_or_else(|| {
            wasmtime::Error::msg("lens::next was called before the module started")
        })?;
    let offset = guest.write(&mut *caller, document.as_deref()).await?;
    let data = caller.data_mut();
    if document.is_some() || !data.input_ended {
        data.input_ended = document.is_none();
        refill(caller.as_context_mut())?;
    }
    Ok(offset)
}
