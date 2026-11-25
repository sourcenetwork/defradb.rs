//! Picks the one chunk document a reader is shown for a hash.
//!
//! Peers that write one text apart and encrypt it each encrypt its chunks with
//! a key of their own, so each copy of a chunk gets its writer's DocID and a
//! peer that receives the other's copy holds both. A reader is shown the copy
//! with the smallest DocID, which every peer picks from the same copies.
//! `proofs/tla/ChunkMerge.tla` checks that a reader sees each chunk once.

use document::{Document, NormalValue};
use rapidhash::RapidHashSet;
use schema::{CollectionVersion, CHUNK_SOURCE_ID_FIELD};

use super::chunk_parent::{chunk_fields, ChunkFields};
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::planner::{IndexScanParams, IndexScanType};

/// The index on `_sourceID` and `hash` every chunk collection has.
pub(crate) fn source_index(chunks_col: &CollectionVersion) -> Option<String> {
    chunks_col
        .indexes
        .iter()
        .find(|index| {
            index
                .fields
                .first()
                .is_some_and(|field| field.name == CHUNK_SOURCE_ID_FIELD)
        })
        .map(|index| index.name.clone())
}

/// The DocID of the copy of `chunk` a reader is shown: the smallest among
/// `doc_id` and the other documents of its parent with `hash` whose text
/// matches it. A copy the reader can't decrypt fails that check.
pub(crate) async fn shown_copy(
    fetcher: &dyn DocFetcher,
    collection: &str,
    index: Option<&str>,
    chunk: &ChunkFields,
    hash: &str,
    doc_id: &str,
) -> Result<String> {
    let smaller: Vec<String> = copies(fetcher, collection, index, chunk, hash)
        .await?
        .into_iter()
        .filter(|copy| copy.as_str() < doc_id)
        .collect();
    if smaller.is_empty() {
        return Ok(doc_id.to_owned());
    }
    let found = match fetcher
        .get_by_ids_projected(collection, &smaller, &chunk_fields())
        .await?
    {
        Some(found) => found,
        None => fetcher.get_by_ids(collection, &smaller).await?,
    };
    Ok(found
        .into_docs()
        .iter()
        .filter(|copy| same_chunk(copy, chunk, hash))
        .filter_map(|copy| copy.id().map(ToString::to_string))
        .min()
        .unwrap_or_else(|| doc_id.to_owned()))
}

fn same_chunk(copy: &Document, chunk: &ChunkFields, hash: &str) -> bool {
    !copy.is_deleted()
        && ChunkFields::from_document(copy).is_some_and(|copy| {
            copy.parent_id == chunk.parent_id && copy.verified_hash() == Some(hash)
        })
}

/// The DocIDs of the documents of `chunk`'s parent with `hash`.
async fn copies(
    fetcher: &dyn DocFetcher,
    collection: &str,
    index: Option<&str>,
    chunk: &ChunkFields,
    hash: &str,
) -> Result<RapidHashSet<String>> {
    if let Some(index) = index.filter(|_| fetcher.supports_index_queries()) {
        let params = IndexScanParams {
            index_name: index.to_owned(),
            scan_type: IndexScanType::PrefixScan {
                prefix_values: vec![
                    NormalValue::String(chunk.parent_id.clone()),
                    NormalValue::String(hash.to_owned()),
                ],
                reverse: false,
            },
            limit: None,
            offset: 0,
            value_filter: None,
            cursor_seek: None,
        };
        let found = fetcher.get_by_index_scan(collection, &params).await?;
        return Ok(found.into_doc_ids().into_iter().collect());
    }
    Ok(fetcher
        .get_by_field_value(collection, CHUNK_SOURCE_ID_FIELD, &chunk.parent_id)
        .await?
        .iter()
        .filter(|copy| same_chunk(copy, chunk, hash))
        .filter_map(|copy| copy.id().map(ToString::to_string))
        .collect())
}
