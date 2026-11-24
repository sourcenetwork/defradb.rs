//! Transaction lifecycle and the transaction registry.
#[path = "../common/mod.rs"]
mod common;

mod guard;
mod index_statement;
mod lifecycle;
mod registry_suite;
mod requests;
mod statement_savepoint;

mod context_ownership;
mod ownership;
mod schema_visibility;
mod store_release;
mod view_statement;
