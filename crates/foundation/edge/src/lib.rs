//! The network edge: the one runtime that serves I/O and networking, held
//! in one place and reached only through a narrow handle.
//!
//! [`EdgeRuntime`] starts the runtime from an [`EdgeConfig`] and shuts it
//! down in one order: stop accepting, drain admitted work with a deadline,
//! stop the rest, drop the runtime outside any async context. Every network
//! component (HTTP, P2P, the KMS transport, the Orbis and Vera clients) is
//! given an [`Edge`] at construction and runs on it; nothing looks a
//! runtime up on its own. A [`Scope`] lets one component stop accepting and
//! drain its own work.
//!
//! Every completion the handle gives back, a [`Task`], carries a deadline,
//! and a panic in network work fails only that work's completion and is
//! counted ([`EdgeStats`]). Long-running work, such as an accept loop or a
//! subscription, runs as a [`Service`] that stops when its runtime or scope
//! stops accepting. Tasks one component owns together, such as a server's
//! connections, run in a [`TaskSet`] that stops them when it is dropped.
//!
//! No type of the runtime underneath appears in this crate's API, so
//! replacing it replaces this crate alone. Natively it is one nuclei
//! runtime; on wasm32 it is the browser's event loop, behind the same
//! handle (without the blocking calls a browser cannot make). A build
//! that starts no networking starts no runtime; a component given no
//! `Edge` that needs one reaches the process's [`on_demand`] runtime,
//! started the first time it is needed.
//!
//! A library that binds its own sockets on tokio, and takes none of ours,
//! is reached through [`compat`], the one place tokio is entered.

#[cfg(not(target_arch = "wasm32"))]
mod compat;
mod config;
mod contain;
mod error;
mod gate;
mod handle;
#[cfg(not(target_arch = "wasm32"))]
mod on_demand;
mod platform;
mod pool;
mod runtime;
mod shared;
mod stats;
mod task;
mod task_set;
#[cfg(feature = "testing")]
pub mod testing;
mod timer;

#[cfg(not(target_arch = "wasm32"))]
pub use compat::{compat, Compat};
pub use config::{EdgeConfig, ThreadStart};
pub use error::EdgeError;
pub use handle::{Drained, Edge, Scope};
#[cfg(not(target_arch = "wasm32"))]
pub use on_demand::on_demand;
#[cfg(target_arch = "wasm32")]
pub use pool::page_pool;
pub use runtime::EdgeRuntime;
pub use stats::EdgeStats;
pub use task::{Service, Task};
pub use task_set::TaskSet;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
