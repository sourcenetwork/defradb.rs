//! The splitter settings a `@chunk` field's definition names.

use chunk_split::split::ChunkConfig;
use schema::ChunkedField;

use crate::error::{Error, Result};

/// The config to cut or check `field`'s chunks with. It fails, rather than cut
/// with other rules, when this node lacks the definition's splitter version.
pub fn chunk_config(collection: &str, field: &ChunkedField) -> Result<ChunkConfig> {
    ChunkConfig::new(
        field.splitter_version,
        field.max_chars as usize,
        field.overlap as usize,
    )
    .map_err(|source| Error::UnknownSplitterVersion {
        field: format!("{collection}.{}", field.field_name),
        source,
    })
}
