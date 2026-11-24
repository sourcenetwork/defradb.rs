//! Shared storage representation used by vector indexes and merge operands.
use crate::index::error::{Error, Result};
use crate::index::vector::store::{Meta, Node};
pub use storage::vector::{encode_meta, encode_node};
pub fn decode_node(bytes: &[u8]) -> Result<Node> {
    storage::vector::decode_node(bytes).map_err(|error| Error::Other(error.to_string()))
}
pub fn decode_meta(bytes: &[u8]) -> Result<Meta> {
    storage::vector::decode_meta(bytes).map_err(|error| Error::Other(error.to_string()))
}
