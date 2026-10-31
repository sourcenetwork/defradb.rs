//! The `@chunk` splitter.
//!
//! It splits the text into paragraphs, groups them into segments, and packs
//! each segment into chunks of at most `max_chars` characters. A chunk can
//! repeat the last sentences of the chunk before it. Lengths count characters,
//! not bytes.
//!
//! The Lean proof `proofs/lean/ChunkSplit` shows that the chunks cover the text
//! with no gaps, that no chunk is too long, and that an edit which keeps its
//! paragraph's fences only changes the chunks of its segment.

mod atoms;
mod overlap;
mod pack;
mod paragraphs;
mod segments;

pub use atoms::{cut_sentences, cut_words};
pub use paragraphs::{paragraphs, Para};
pub use segments::fence_before;

use crate::list::text_hash;

/// Settings for one `@chunk` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkConfig {
    /// The most characters a chunk may hold, overlap included.
    pub max_chars: usize,
    /// How many sentences a chunk repeats from the end of the chunk before it.
    pub overlap: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Start of the chunk's own text in the parent text, in characters.
    pub start: usize,
    /// End of the chunk's own text. These ranges cover the whole text with no gaps.
    pub end: usize,
    /// What the chunk stores, which is any repeated sentences and then its own text.
    pub text: String,
    /// Hex SHA-256 of `text`, the chunk's identity.
    pub hash: String,
}

pub fn split(text: &str, config: &ChunkConfig) -> Vec<Chunk> {
    let max = config.max_chars.max(1);
    let paras = paragraphs(text);
    let mut chunks = Vec::new();
    for segment in segments::segments(&paras, max) {
        let start = segment[0].start;
        let atoms: Vec<&str> = segment
            .iter()
            .flat_map(|para| atoms::atoms(para.text, max))
            .collect();
        let cores = pack::pack(&atoms, max);
        let mut offset = start;
        let prefixes = overlap::with_overlap(&cores, max, config.overlap);
        for (pre, core) in prefixes.into_iter().zip(&cores) {
            let len = core.chars().count();
            let mut stored = String::with_capacity(pre.len() + core.len());
            stored.push_str(pre);
            stored.push_str(core);
            chunks.push(Chunk {
                start: offset,
                end: offset + len,
                hash: text_hash(&stored),
                text: stored,
            });
            offset += len;
        }
    }
    chunks
}
