//! Chunk identities and the chunk documents a text needs.

use rapidhash::{HashSetExt, RapidHashSet};
use sha2::{Digest, Sha256};

use super::split::Chunk;

/// Hex SHA-256 of `text`, used as a chunk's identity.
pub fn text_hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// The chunks that need a document, each distinct hash once, in reading order.
pub fn distinct(chunks: &[Chunk]) -> Vec<&Chunk> {
    let mut seen = RapidHashSet::with_capacity(chunks.len());
    chunks
        .iter()
        .filter(|chunk| seen.insert(chunk.hash.as_str()))
        .collect()
}
