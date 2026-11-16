//! Checks whether a parent's current text uses a chunk.

use chunk_split::list::{describes, text_hash};
use chunk_split::split::{split, ChunkConfig};
use document::{Document, NormalValue};
use rapidhash::RapidHashSet;
use schema::{ChunkedField, CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD};

/// The fields of a chunk the read check uses.
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

/// Whether `parent` is live and its current text uses `chunk`.
#[cfg(test)]
fn parent_lists(parent: &Document, chunk: &Document, chunked: &ChunkedField) -> bool {
    let chunk = ChunkFields::from_document(chunk);
    match (
        chunk.as_ref().and_then(ChunkFields::verified_hash),
        parent_hashes(parent, chunked),
    ) {
        (Some(hash), Some(hashes)) => hashes.contains(hash),
        _ => false,
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
            // A splitter version this node lacks can't check the text, so nothing shows.
            let config = ChunkConfig::new(
                chunked.splitter_version,
                chunked.max_chars as usize,
                chunked.overlap as usize,
            )
            .ok()?;
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
