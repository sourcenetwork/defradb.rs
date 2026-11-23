//! Text chunking: chunk schemas, guards, windows, writes and key owners.
#[path = "../common/mod.rs"]
mod common;

mod admin_ops;
mod config;
mod delete;
mod follow;
mod guard;
mod key_owners;
mod kms_release;
mod lookup;
mod parent_collection;
mod race;
mod read;
mod schema;
mod signed;
mod sync;
mod truncate;
mod user_writes;
mod verify;
mod window;
