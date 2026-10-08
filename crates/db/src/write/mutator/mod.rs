//! The document mutators. They differ only in who owns the transaction and
//! what they lock; the writes themselves live in the operation modules.

pub mod autocommit;
pub mod batch;
pub mod txn;
