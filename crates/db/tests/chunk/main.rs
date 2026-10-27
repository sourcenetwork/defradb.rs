//! Text chunking: chunk schemas, windows, writes and key owners.
#[path = "../common/mod.rs"]
mod common;

mod key_owners;
mod race;
mod schema;
mod user_writes;
mod window;
