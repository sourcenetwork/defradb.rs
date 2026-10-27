//! `@chunk` settings, on the parent collection and on its chunk collection.

use serde::{Deserialize, Serialize};

/// The relation field on a chunk that points at its parent.
pub const CHUNK_SOURCE_FIELD: &str = "source";
/// The foreign key the relation field stores.
pub const CHUNK_SOURCE_ID_FIELD: &str = "_sourceID";
/// The chunk's text, including any sentences repeated from the chunk before.
pub const CHUNK_TEXT_FIELD: &str = "text";
/// Hex SHA-256 of the chunk's text, its identity within the parent.
pub const CHUNK_HASH_FIELD: &str = "hash";

/// Name of the collection generated for `field` of `collection`.
pub fn chunk_collection_name(collection: &str, field: &str) -> String {
    format!("{collection}__{field}_chunk")
}

/// Name of the parent's generated list field for `field`.
pub fn chunk_list_field(field: &str) -> String {
    format!("_{field}_chunks")
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
}

impl ChunkedField {
    /// The parent's generated list field for this text field.
    pub fn list_field(&self) -> String {
        chunk_list_field(&self.field_name)
    }

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
