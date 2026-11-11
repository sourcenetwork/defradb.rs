//! Transaction lifecycle and the transaction registry.
#[path = "../common/mod.rs"]
mod common;

mod guard;
mod lifecycle;
mod registry_suite;
mod requests;
mod statement_savepoint;

mod context_ownership;
mod ownership;
mod patch_in_txn;
mod schema_visibility;
mod store_release;
