//! The chunk list stored on the parent.
//!
//! The first entry is the hash of the whole text, so a reader can tell when the
//! list was made from a different text. The other entries are the chunk hashes
//! in reading order. A repeated chunk is listed once for each place it appears.

use rapidhash::{HashSetExt, RapidHashSet};
use sha2::{Digest, Sha256};

use super::split::Chunk;

/// Hex SHA-256 of `text`, used as a chunk's identity.
pub fn text_hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// The list to store on the parent for `text`.
pub fn chunk_list(text: &str, chunks: &[Chunk]) -> Vec<String> {
    let mut list = Vec::with_capacity(chunks.len() + 1);
    list.push(text_hash(text));
    list.extend(chunks.iter().map(|chunk| chunk.hash.clone()));
    list
}

/// The chunks that need a document, each distinct hash once, in reading order.
pub fn distinct(chunks: &[Chunk]) -> Vec<&Chunk> {
    let mut seen = RapidHashSet::with_capacity(chunks.len());
    chunks
        .iter()
        .filter(|chunk| seen.insert(chunk.hash.as_str()))
        .collect()
}

/// Whether `list` was made from `text`.
pub fn describes(list: &[String], text: &str) -> bool {
    list.first().is_some_and(|first| *first == text_hash(text))
}
