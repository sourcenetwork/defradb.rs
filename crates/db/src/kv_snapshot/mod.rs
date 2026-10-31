//! KV snapshots: a collection's documents as a stream of key-value frames in
//! Go Shinzo's layout, exported from one node and imported into another
//! (`go.shinzo.kv_snapshot`). defradb.rs writes Go's keys from its own state
//! and imports by replaying the documents' blocks through the merge.

mod error;
mod export;
pub mod frame;
mod go_keys;
mod import;
mod mapping;

pub use error::KvSnapshotError;
pub use export::{ExportOptions, Exported};
pub use import::Imported;
pub use mapping::{field_entries, DocumentEntry, FieldEntry, FieldMapping};
