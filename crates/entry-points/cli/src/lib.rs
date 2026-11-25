//! DefraDB CLI library
//!
//! This library provides the core functionality for the DefraDB CLI.
//! It is primarily used by the `defra` binary but can also be used for testing.

pub(crate) mod backup_adapter;
#[cfg(test)]
#[path = "backup_adapter_tests.rs"]
mod backup_adapter_tests;
pub(crate) mod block_adapter;
pub mod cli;
pub mod commands;
pub mod config;
pub(crate) mod doc_acp_adapter;
pub(crate) mod dump_adapter;
pub(crate) mod encrypted_index_adapter;
pub mod error;
pub mod logging;
#[cfg(test)]
#[path = "nac_adapter_tests.rs"]
mod nac_adapter_tests;
pub(crate) mod schema_adapter;
pub(crate) mod view_adapter;
