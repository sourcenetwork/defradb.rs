//! Go Shinzo's features (#2435) on the C ABI: each function only translates
//! its arguments and errors for the core.

mod kv;
mod node_options;
mod peer_key;
mod purge;
mod save_many;

pub use kv::{kv_export, kv_field_mapping, kv_import};
pub use node_options::{with_node_options, DefraReplicationFilterCallback};
pub(crate) use peer_key::peer_keyring;
pub use purge::purge_documents;
pub use save_many::save_many_documents;

#[cfg(test)]
mod tests;
