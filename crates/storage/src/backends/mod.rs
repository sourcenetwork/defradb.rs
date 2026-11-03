//! Storage backends.
//!
//! There is one: regolith. It is the only store on every target DefraDB
//! runs on, so there is nothing here to select between and no feature
//! flag to get wrong.

pub(crate) mod shared;

pub mod regolith;

// Public rather than `cfg(test)`: the suite is the backend contract, and an
// integration test links the library without its test cfg, through the
// `test-suite` feature. Gated off wasm because it drives concurrent
// transactions from worker tasks.
#[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-suite")))]
pub mod test_suite;

#[cfg(not(target_arch = "wasm32"))]
pub use regolith::start_pool;
pub use regolith::{with_worker_io, RegolithStore, RegolithStoreOptions, RegolithTxn, WorkerIo};
pub use shared::{DurabilityMode, TransactionStatsHandle, TransactionStatsSnapshot};
