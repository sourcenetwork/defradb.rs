//! Defradb-specific Zanzibar store implementations.

mod persistent;

pub use persistent::PersistentZanzibarStore;

mod transaction;
pub use transaction::TransactionZanzibarStore;
