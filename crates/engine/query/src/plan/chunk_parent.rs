//! Checks whether a parent's current text uses a chunk.

use chunk_split::hash::text_hash;
use chunk_split::split::{split, ChunkConfig};
use document::{Document, NormalValue};
use rapidhash::RapidHashSet;
use schema::{ChunkedField, CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD};

pub(crate) struct ChunkFields {
    pub(crate) parent_id: String,
    text: String,
    hash: String,
}

impl ChunkFields {
    pub(crate) fn new(parent_id: &str, text: &str, hash: &str) -> Self {
        Self {
            parent_id: parent_id.to_owned(),
            text: text.to_owned(),
            hash: hash.to_owned(),
        }
    }

    pub(crate) fn from_document(chunk: &Document) -> Option<Self> {
        let field = |name| chunk.get(name).and_then(NormalValue::as_str);
        Some(Self::new(
            field(CHUNK_SOURCE_ID_FIELD)?,
            field(CHUNK_TEXT_FIELD)?,
            field(CHUNK_HASH_FIELD)?,
        ))
    }

    /// The chunk's hash, if it matches its text, so a chunk can't claim another's hash.
    pub(crate) fn verified_hash(&self) -> Option<&str> {
        (text_hash(&self.text) == self.hash).then_some(self.hash.as_str())
    }
}

/// The hashes of the chunks a live parent's current text uses.
pub(crate) fn parent_hashes(
    parent: &Document,
    chunked: &ChunkedField,
) -> Option<RapidHashSet<String>> {
    if parent.is_deleted() {
        return None;
    }
    let parent_text = parent
        .get(&chunked.field_name)
        .and_then(NormalValue::as_str)?;
    text_hashes(parent_text, chunked)
}

/// The hashes of the chunks `text` cuts into, or `None` when this node
/// can't run the field's splitter.
pub(crate) fn text_hashes(text: &str, chunked: &ChunkedField) -> Option<RapidHashSet<String>> {
    let config = match ChunkConfig::new(
        chunked.splitter_version,
        chunked.max_chars as usize,
        chunked.overlap as usize,
    ) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(
                field = %chunked.field_name,
                splitter_version = chunked.splitter_version,
                %error,
                "chunk splitter not supported on this node; this field's chunks are hidden"
            );
            return None;
        }
    };
    Some(
        split(text, &config)
            .into_iter()
            .map(|chunk| chunk.hash)
            .collect(),
    )
}

/// The parent a chunk names.
pub(crate) fn chunk_parent_id(chunk: &Document) -> Option<&str> {
    chunk
        .get(CHUNK_SOURCE_ID_FIELD)
        .and_then(NormalValue::as_str)
}

/// The chunk's hash, if it matches its text, so a chunk can't claim another's hash.
pub(crate) fn verified_hash(chunk: &Document) -> Option<&str> {
    let text = chunk.get(CHUNK_TEXT_FIELD).and_then(NormalValue::as_str)?;
    let hash = chunk.get(CHUNK_HASH_FIELD).and_then(NormalValue::as_str)?;
    (text_hash(text) == hash).then_some(hash)
}

pub(crate) fn chunk_fields() -> RapidHashSet<String> {
    [CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD, CHUNK_HASH_FIELD]
        .into_iter()
        .map(String::from)
        .collect()
}

pub(crate) fn parent_fields(chunked: &ChunkedField) -> RapidHashSet<String> {
    [chunked.field_name.clone()].into_iter().collect()
}

#[cfg(test)]
#[path = "chunk_parent_tests.rs"]
mod tests;
