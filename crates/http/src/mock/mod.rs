//! Mock executors for testing HTTP routes.

use kovan::Atom;

mod acp;
mod backup;
mod block;
mod collections;
mod doc_acp;
mod encrypted_index;
mod index;
mod lens;
mod nac;
mod p2p;
mod query;
mod rest;
mod txn_ops;

pub use acp::{FailingMockAcpOperations, MockAcpOperations};
pub use backup::{FailingMockBackupOperations, MockBackupOperations};
pub use block::MockBlockOperations;
pub use collections::MockCollectionManagementOperations;
pub use doc_acp::MockDocumentAcpOperations;
pub use encrypted_index::MockEncryptedIndexOperations;
pub use index::{FailingMockIndexOperations, MockIndexOperations};
pub use lens::MockLensOperations;
pub use nac::{FailingMockNodeAcpOperations, MockNodeAcpOperations};
pub use p2p::{FailingMockP2POperations, MockP2POperations};
pub use query::{FailingMockExecutor, MockQueryExecutor};
pub use rest::{FailingMockRestOperations, MockRestOperations};
pub use txn_ops::MockTransactionOperations;

/// Applies `f` to a private copy of `atom`'s vector and publishes it, retrying
/// until no concurrent writer replaced the value in between.
fn update_vec<T, R>(atom: &Atom<Vec<T>>, mut f: impl FnMut(&mut Vec<T>) -> R) -> R
where
    T: Clone + Send + Sync + 'static,
{
    loop {
        let current = atom.load();
        let mut next = (*current).clone();
        let result = f(&mut next);
        if atom.compare_and_swap(&current, next).is_ok() {
            return result;
        }
    }
}
