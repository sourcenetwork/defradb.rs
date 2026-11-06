//! Transaction management for query execution.
//!
//! Plan-layer primitives (`TransactionContext`, `TransactionHandle`,
//! `TransactionRegistry` and the request's `TxnTurn`) live in the sibling
//! modules here. `TransactionGuard` lives in `guard`; it is generic over the
//! runner-layer `QueryExecutor` trait.

mod collections;
mod context;
mod guard;
mod handle;
mod read_access;
mod registry;
mod source_nac;
mod turn;

#[cfg(test)]
mod tests;

pub use collections::TxnCollections;
pub use context::{
    check_doc_access_with_prepared_inputs, check_doc_access_with_projection,
    check_doc_read_access_as, check_doc_read_access_with_prepared_inputs,
    is_doc_registered_with_prepared_inputs, is_doc_registered_with_projection,
    AcpRegistrationProjection, TransactionContext,
};
pub use guard::TransactionGuard;
pub use handle::TransactionHandle;
pub use read_access::OverlayChecker;
pub use registry::{NoOpTransactionRegistry, TransactionRegistry};
pub use source_nac::{host_source_permission_job, source_permission_job, SourcePermission};
pub use turn::{txn_job, SharedTurn, TxnJob, TxnTurn};
