//! Go Shinzo's core data features (#2439). A test named `shinzo_` compares
//! with a Go Shinzo node and also runs under `DEFRA_CLIENT=go-shinzo`; the
//! others configure what only a Rust node can be given.

#[path = "shinzo/purge.rs"]
mod purge;

#[path = "shinzo/save_many.rs"]
mod save_many;

#[path = "shinzo/kv_snapshot.rs"]
mod kv_snapshot;

#[path = "shinzo/cursor.rs"]
mod cursor;

#[path = "shinzo/merge.rs"]
mod merge;

#[path = "shinzo/unique_index.rs"]
mod unique_index;
