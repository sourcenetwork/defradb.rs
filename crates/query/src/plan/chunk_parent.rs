//! Checks whether a parent's current text uses a chunk.

use chunk_split::list::{describes, text_hash};
use chunk_split::split::{split, ChunkConfig};
use document::{Document, NormalValue};
use rapidhash::RapidHashSet;
use schema::{ChunkedField, CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD};

/// The parent DocID stored on a chunk.
pub(crate) fn chunk_parent_id(chunk: &Document) -> Option<&str> {
    chunk
        .get(CHUNK_SOURCE_ID_FIELD)
        .and_then(NormalValue::as_str)
}

/// Whether `parent` is live and its current text uses `chunk`.
pub(crate) fn parent_lists(parent: &Document, chunk: &Document, chunked: &ChunkedField) -> bool {
    match (verified_hash(chunk), parent_hashes(parent, chunked)) {
        (Some(hash), Some(hashes)) => hashes.contains(hash),
        _ => false,
    }
}

/// The chunk's hash, if it matches its text, so a chunk can't claim another's hash.
pub(crate) fn verified_hash(chunk: &Document) -> Option<&str> {
    let hash = chunk.get(CHUNK_HASH_FIELD).and_then(NormalValue::as_str)?;
    let text = chunk.get(CHUNK_TEXT_FIELD).and_then(NormalValue::as_str)?;
    (text_hash(text) == hash).then_some(hash)
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
    let list = match parent.get(&chunked.list_field()) {
        Some(NormalValue::StringArray(list)) => Some(list),
        Some(NormalValue::NillableStringArray(Some(list))) => Some(list),
        _ => None,
    };
    Some(match list {
        // The first entry is the whole text's hash, not a chunk.
        Some(list) if describes(list, parent_text) => list.iter().skip(1).cloned().collect(),
        // Text and list merge as separate fields, so concurrent writes can leave one
        // peer's text with the other's list. Split the text again to be sure.
        _ => {
            let config = ChunkConfig {
                max_chars: chunked.max_chars as usize,
                overlap: chunked.overlap as usize,
            };
            split(parent_text, &config)
                .into_iter()
                .map(|chunk| chunk.hash)
                .collect()
        }
    })
}

/// The fields of a chunk the read check uses.
pub(crate) fn chunk_fields() -> RapidHashSet<String> {
    [CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD, CHUNK_HASH_FIELD]
        .into_iter()
        .map(String::from)
        .collect()
}

/// The fields of a parent the read check uses.
pub(crate) fn parent_fields(chunked: &ChunkedField) -> RapidHashSet<String> {
    [chunked.field_name.clone(), chunked.list_field()]
        .into_iter()
        .collect()
}

#[cfg(test)]
#[path = "chunk_parent_tests.rs"]
mod tests;
