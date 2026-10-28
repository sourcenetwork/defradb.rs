//! Single-threaded channels, a semaphore and `select!` with the names and
//! semantics of the nuclei and tokio ones p2p uses. nuclei has no wasm32 build
//! and no crate of ours may depend on tokio, so the browser build runs on
//! these. The browser has one thread: state is a `RefCell` plus wakers, and
//! only there are the halves `Send` and `Sync`. The module
//! compiles on every target so its tests run natively, where nothing here is
//! `Sync`.

pub mod mpsc;
pub mod oneshot;
pub mod select;
pub mod semaphore;
pub mod watch;

pub use semaphore::{OwnedSemaphorePermit, Semaphore};

/// State shared by the halves of one primitive, which are all used on the
/// one thread of the browser.
///
/// p2p's transport traits require `Send + Sync` of everything they hold, so the
/// halves must claim both although none is ever touched from a second thread.
pub(crate) struct OneThread<T>(pub(crate) T);

// SAFETY: wasm32 without the atomics feature has exactly one thread, so the
// interior mutability inside is never reached from two threads at once. Any
// target with threads gets no such claim.
#[cfg(all(target_arch = "wasm32", not(target_feature = "atomics")))]
unsafe impl<T: Send> Send for OneThread<T> {}
// SAFETY: as above.
#[cfg(all(target_arch = "wasm32", not(target_feature = "atomics")))]
unsafe impl<T: Send> Sync for OneThread<T> {}

impl<T> std::ops::Deref for OneThread<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}
