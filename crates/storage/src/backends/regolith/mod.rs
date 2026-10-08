//! regolith-backed storage: the one backend, on every target.

mod background_errors;
mod config;
mod handle;
// DEFRALEVEL(S2): Remove this declaration together with head_cache.rs; S2 retires the collection head cache.
mod head_cache;
pub(crate) mod iterator;
mod store;
// DEFRALEVEL(S1): Add mod policy: DefraKeyClassifier; h/c/{id}/, h/cs/{id}/ CommutativePrefix (canonical id); else Ordinary
mod transaction;

pub use config::RegolithStoreOptions;
pub use store::RegolithStore;
pub use transaction::RegolithTxn;

// DEFRALEVEL(S9): Delete blocking(): begin/commit/streaming_writer/drop_all run on the owning core-pinned worker; call native ops
/// Native storage and publication locks may wait synchronously. Let Tokio
/// replace the worker while keeping the operation on this stack: detaching a
/// commit would allow cancellation to release its caller's serialization guard
/// before native commit and cache publication finish. Other executors retain
/// the synchronous backend contract.
fn blocking<T>(operation: impl FnOnce() -> T) -> T {
    #[cfg(not(target_arch = "wasm32"))]
    if tokio::runtime::Handle::try_current()
        .is_ok_and(|handle| handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread)
    {
        return tokio::task::block_in_place(operation);
    }
    operation()
}
