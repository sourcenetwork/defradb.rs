//! Schema loading, patching, lens migration and index format.
#[path = "../common/mod.rs"]
mod common;

mod committed_lenses;
mod counter_migration;
mod document_parts;
mod format;
mod helpers;
mod json;
mod loader;
