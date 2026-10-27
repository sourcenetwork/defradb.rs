//! Text chunking: chunk schemas, windows, chunk documents and key owners.
#[path = "../common/mod.rs"]
mod common;

mod guard;
mod key_owners;
mod race;
mod read;
mod schema;
mod sync;
mod truncate;
mod user_writes;
mod window;
