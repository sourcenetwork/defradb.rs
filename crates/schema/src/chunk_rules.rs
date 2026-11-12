//! Rules a version's `@chunk` settings follow, whether it was parsed here or
//! learned from a peer's definition block.

use std::fmt;

use defra_core::DEFAULT_SPLITTER_VERSION;
use rapidhash::{HashSetExt, RapidHashSet};
use thiserror::Error;

use crate::{
    chunk_collection_name, CType, ChunkedField, CollectionVersion, FieldKind, CHUNK_HASH_FIELD,
    CHUNK_SOURCE_FIELD, CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD,
};

/// Smaller chunks carry too little text to embed or rank well.
pub const MIN_CHUNK_MAX_CHARS: u32 = 64;

/// The fields a chunk collection's parent writes into every chunk.
const GENERATED_CHUNK_FIELDS: [&str; 4] = [
    CHUNK_SOURCE_FIELD,
    CHUNK_SOURCE_ID_FIELD,
    CHUNK_TEXT_FIELD,
    CHUNK_HASH_FIELD,
];

/// A `@chunk` setting no schema could have declared, or one a later version changed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ChunkSettingsError {
    #[error(
        "@chunk maxChars must be at least {MIN_CHUNK_MAX_CHARS}, got {max_chars}. Field: {field}"
    )]
    MaxCharsTooSmall { field: String, max_chars: u32 },

    #[error("@chunk is set twice. Field: {field}")]
    DuplicateField { field: String },

    #[error("@chunk is not allowed on a field whose name starts with \"_\". Field: {field}")]
    UnderscoreField { field: String },

    #[error("@chunk is only allowed on a last-writer-wins String field. Field: {field}")]
    NotLwwString { field: String },

    #[error("@chunk needs the list field {list}. Field: {field}")]
    MissingListField { field: String, list: String },

    #[error("@chunk is not allowed in a view. Field: {field}")]
    InView { field: String },

    #[error("{collection} names {parent}.{parent_field} as its parent, whose chunk collection is {expected}")]
    WrongChunkCollection {
        collection: String,
        parent: String,
        parent_field: String,
        expected: String,
    },

    #[error("a chunk collection cannot have @chunk fields of its own. Collection: {collection}")]
    ChunksOfChunks { collection: String },

    #[error("chunk collection {collection} lacks its generated field {field}")]
    MissingGeneratedField {
        collection: String,
        field: &'static str,
    },

    #[error("{collection} names {parent}.{parent_field} as its parent, which has no @chunk")]
    ParentNotChunked {
        collection: String,
        parent: String,
        parent_field: String,
    },

    #[error("chunk settings cannot change between versions. Collection: {collection}, Version: {version_id}, Changed: {changed}")]
    Changed {
        collection: String,
        version_id: String,
        changed: ChunkSettingsChange,
    },
}

/// Which part of the `@chunk` settings two versions disagree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkSettingsChange {
    /// The parent's `@chunk` fields or their `maxChars` and `overlap`.
    ChunkedFields,
    /// The splitting rules of a `@chunk` field.
    SplitterVersion,
    /// The parent field a chunk collection is made from.
    ChunkOf,
}

impl fmt::Display for ChunkSettingsChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ChunkedFields => "the @chunk fields",
            Self::SplitterVersion => "the @chunk splitter version",
            Self::ChunkOf => "the chunk parent",
        })
    }
}

/// Refuses a `maxChars` below [`MIN_CHUNK_MAX_CHARS`]. `field` is written `Type.field`.
pub fn ensure_chunk_max_chars(field: &str, max_chars: u32) -> Result<(), ChunkSettingsError> {
    if max_chars < MIN_CHUNK_MAX_CHARS {
        return Err(ChunkSettingsError::MaxCharsTooSmall {
            field: field.to_string(),
            max_chars,
        });
    }
    Ok(())
}

/// What `next` changes in the `@chunk` settings of `previous`, if anything.
pub fn chunk_settings_change(
    previous: &CollectionVersion,
    next: &CollectionVersion,
) -> Option<ChunkSettingsChange> {
    let (before, after) = (
        sorted_by_name(&previous.chunked_fields),
        sorted_by_name(&next.chunked_fields),
    );
    if !same_ignoring_version(&before, &after) {
        Some(ChunkSettingsChange::ChunkedFields)
    } else if before != after {
        Some(ChunkSettingsChange::SplitterVersion)
    } else if previous.chunk_of != next.chunk_of {
        Some(ChunkSettingsChange::ChunkOf)
    } else {
        None
    }
}

fn same_ignoring_version(left: &[&ChunkedField], right: &[&ChunkedField]) -> bool {
    let unversioned = |field: &&ChunkedField| ChunkedField {
        splitter_version: DEFAULT_SPLITTER_VERSION,
        ..(*field).clone()
    };
    left.iter()
        .map(unversioned)
        .eq(right.iter().map(unversioned))
}

/// Order doesn't count: versions stored before the parser sorted them keep SDL order.
fn sorted_by_name(fields: &[ChunkedField]) -> Vec<&ChunkedField> {
    let mut fields: Vec<&ChunkedField> = fields.iter().collect();
    fields.sort_by(|a, b| a.field_name.cmp(&b.field_name));
    fields
}

/// Refuses `next` unless it keeps the `@chunk` settings of `previous`, because
/// existing chunks were cut with them.
pub fn ensure_chunk_settings_kept(
    previous: &CollectionVersion,
    next: &CollectionVersion,
) -> Result<(), ChunkSettingsError> {
    match chunk_settings_change(previous, next) {
        Some(changed) => Err(ChunkSettingsError::Changed {
            collection: next.name.clone(),
            version_id: next.version_id.clone(),
            changed,
        }),
        None => Ok(()),
    }
}

/// The `@chunk` rules a collection's first version breaks. The SDL never
/// produces these, so they come from a hand-built definition or a peer's block.
pub fn chunk_settings_errors(version: &CollectionVersion) -> Vec<ChunkSettingsError> {
    let mut errors = Vec::new();
    let is_view = version.query.is_some() || version.downsample_source.is_some();
    let mut seen = RapidHashSet::with_capacity(version.chunked_fields.len());
    for chunked in &version.chunked_fields {
        let field = format!("{}.{}", version.name, chunked.field_name);
        if let Err(error) = ensure_chunk_max_chars(&field, chunked.max_chars) {
            errors.push(error);
        }
        if !seen.insert(chunked.field_name.as_str()) {
            errors.push(ChunkSettingsError::DuplicateField {
                field: field.clone(),
            });
        }
        if chunked.field_name.starts_with('_') {
            errors.push(ChunkSettingsError::UnderscoreField {
                field: field.clone(),
            });
        }
        let is_lww_string = version
            .field_by_name(&chunked.field_name)
            .is_some_and(|text| {
                text.kind == FieldKind::string()
                    && matches!(text.crdt_type, CType::None | CType::LwwRegister)
            });
        if !is_lww_string {
            errors.push(ChunkSettingsError::NotLwwString {
                field: field.clone(),
            });
        }
        let list = chunked.list_field();
        if version.field_by_name(&list).is_none() {
            errors.push(ChunkSettingsError::MissingListField {
                field: field.clone(),
                list,
            });
        }
        if is_view {
            errors.push(ChunkSettingsError::InView { field });
        }
    }
    if let Some(parent) = &version.chunk_of {
        let expected = chunk_collection_name(&parent.collection, &parent.field_name);
        if version.name != expected {
            errors.push(ChunkSettingsError::WrongChunkCollection {
                collection: version.name.clone(),
                parent: parent.collection.clone(),
                parent_field: parent.field_name.clone(),
                expected,
            });
        }
        if !version.chunked_fields.is_empty() {
            errors.push(ChunkSettingsError::ChunksOfChunks {
                collection: version.name.clone(),
            });
        }
        for field in GENERATED_CHUNK_FIELDS {
            if version.field_by_name(field).is_none() {
                errors.push(ChunkSettingsError::MissingGeneratedField {
                    collection: version.name.clone(),
                    field,
                });
            }
        }
    }
    errors
}

/// Refuses `chunks` unless `parent`, a version of the collection its chunks
/// point at, declares `@chunk` on the field `chunks` names.
pub fn ensure_chunk_parent(
    parent: &CollectionVersion,
    chunks: &CollectionVersion,
) -> Result<(), ChunkSettingsError> {
    let Some(named) = &chunks.chunk_of else {
        return Ok(());
    };
    if parent.name == named.collection && parent.chunked_field(&named.field_name).is_some() {
        return Ok(());
    }
    Err(ChunkSettingsError::ParentNotChunked {
        collection: chunks.name.clone(),
        parent: named.collection.clone(),
        parent_field: named.field_name.clone(),
    })
}
