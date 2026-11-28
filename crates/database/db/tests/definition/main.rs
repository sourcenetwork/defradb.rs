//! Schema loading, patching, lens migration and index format.
#[path = "../common/mod.rs"]
mod common;

mod committed_lenses;
mod counter_migration;
mod document_parts;
mod format;
mod format_json;
mod helpers;
mod keyspace;
mod lens_commit_window;
mod loader;
