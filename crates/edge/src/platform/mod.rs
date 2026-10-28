//! What differs between targets: how work is spawned, how a timer is made,
//! and who owns the event loop. Natively one nuclei runtime on an owner
//! thread; on wasm32 the browser's event loop.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{wait_here, Instant, JoinHandle, Owner, Rt, Sleep};
#[cfg(target_arch = "wasm32")]
pub(crate) use wasm::{Instant, JoinHandle, Owner, Rt, Sleep};
