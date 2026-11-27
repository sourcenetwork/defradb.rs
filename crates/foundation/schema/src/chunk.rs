//! `@chunk` settings, on the parent collection and on its chunk collection.

use defra_core::{
    is_default_splitter_version, ChunkOfDelta, ChunkedFieldDelta, DEFAULT_SPLITTER_VERSION,
};
use serde::{Deserialize, Serialize};

/// The relation field on a chunk that points at its parent.
pub const CHUNK_SOURCE_FIELD: &str = "source";
/// The foreign key the relation field stores.
pub const CHUNK_SOURCE_ID_FIELD: &str = "_sourceID";
/// The chunk's text, including any sentences repeated from the chunk before.
pub const CHUNK_TEXT_FIELD: &str = "text";
/// Hex SHA-256 of the chunk's text, its identity within the parent.
pub const CHUNK_HASH_FIELD: &str = "hash";
/// How many times the parent's chunk of this hash was deleted before this one
/// was made. A deleted ID can't be used again, so each remake needs its own.
/// The first is unset, so its ID does not change.
pub const CHUNK_GENERATION_FIELD: &str = "generation";

/// Name of the collection generated for `field` of `collection`.
pub fn chunk_collection_name(collection: &str, field: &str) -> String {
    format!("{collection}__{field}_chunk")
}

/// A `@chunk` field, as the parent collection records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkedField {
    /// The text field that is chunked.
    #[serde(rename = "Field")]
    pub field_name: String,
    /// The most characters a chunk may hold.
    #[serde(rename = "MaxChars")]
    pub max_chars: u32,
    /// How many sentences each chunk repeats from the chunk before it.
    #[serde(rename = "Overlap")]
    pub overlap: u32,
    /// The splitting rules its chunks are cut with.
    #[serde(
        rename = "SplitterVersion",
        default = "default_splitter_version",
        skip_serializing_if = "is_default_splitter_version"
    )]
    pub splitter_version: u32,
}

fn default_splitter_version() -> u32 {
    DEFAULT_SPLITTER_VERSION
}

impl ChunkedField {
    /// The generated collection that holds this field's chunks.
    pub fn collection(&self, parent: &str) -> String {
        chunk_collection_name(parent, &self.field_name)
    }
}

/// The parent field a chunk collection is made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkSource {
    /// The parent collection's name.
    #[serde(rename = "Collection")]
    pub collection: String,
    /// The parent's text field the chunks are cut from.
    #[serde(rename = "Field")]
    pub field_name: String,
}

impl From<&ChunkedField> for ChunkedFieldDelta {
    fn from(chunked: &ChunkedField) -> Self {
        Self {
            field: chunked.field_name.clone(),
            max_chars: chunked.max_chars,
            overlap: chunked.overlap,
            splitter_version: chunked.splitter_version,
        }
    }
}

impl From<&ChunkedFieldDelta> for ChunkedField {
    fn from(delta: &ChunkedFieldDelta) -> Self {
        Self {
            field_name: delta.field.clone(),
            max_chars: delta.max_chars,
            overlap: delta.overlap,
            splitter_version: delta.splitter_version,
        }
    }
}

impl From<&ChunkSource> for ChunkOfDelta {
    fn from(source: &ChunkSource) -> Self {
        Self {
            collection: source.collection.clone(),
            field: source.field_name.clone(),
        }
    }
}

impl From<&ChunkOfDelta> for ChunkSource {
    fn from(delta: &ChunkOfDelta) -> Self {
        Self {
            collection: delta.collection.clone(),
            field_name: delta.field.clone(),
        }
    }
}
