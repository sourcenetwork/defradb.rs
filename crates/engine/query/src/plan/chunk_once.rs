//! Picks the one chunk document a reader is shown for a hash.
//!
//! Peers that write one text apart and encrypt it each encrypt its chunks with
//! a key of their own, so each copy of a chunk gets its writer's DocID and a
//! peer that receives the other's copy holds both. A reader is shown the copy
//! it can decrypt with the smallest DocID. Readers with different keys can
//! therefore see different DocIDs, and a shown DocID can change when a smaller
//! readable copy arrives. Other stored copies are hidden from direct lookup.
//! `proofs/tla/ChunkMerge.tla` checks that a reader sees each chunk once.

use std::sync::Arc;

use document::{Document, NormalValue};
use rapidhash::{RapidHashMap, RapidHashSet};
use schema::{CollectionVersion, CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD};

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
            index.fields.first().map(|field| field.name.as_str()) == Some(CHUNK_SOURCE_ID_FIELD)
                && index.fields.get(1).map(|field| field.name.as_str()) == Some(CHUNK_HASH_FIELD)
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
    smallest_readable(fetcher, collection, chunk, hash, doc_id, smaller).await
}

/// Whether `doc_id` is the smallest readable candidate. The current row has
/// already passed the hash check, so only smaller candidates need a fetch.
pub(crate) async fn shown_copy_from_candidates(
    fetcher: &dyn DocFetcher,
    collection: &str,
    chunk: &ChunkFields,
    hash: &str,
    doc_id: &str,
    candidates: &[Arc<str>],
) -> Result<bool> {
    let smaller: Vec<String> = candidates
        .iter()
        .filter(|copy| copy.as_ref() < doc_id)
        .map(ToString::to_string)
        .collect();
    if smaller.is_empty() {
        return Ok(true);
    }
    Ok(smallest_readable(fetcher, collection, chunk, hash, doc_id, smaller).await? == doc_id)
}

async fn smallest_readable(
    fetcher: &dyn DocFetcher,
    collection: &str,
    chunk: &ChunkFields,
    hash: &str,
    doc_id: &str,
    smaller: Vec<String>,
) -> Result<String> {
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

/// The DocIDs of every copy of `parent_id`, by chunk hash.
pub(crate) async fn all_copies(
    fetcher: &dyn DocFetcher,
    collection: &str,
    index: Option<&str>,
    parent_id: &str,
) -> Result<RapidHashMap<Arc<str>, Arc<[Arc<str>]>>> {
    let index = index.filter(|_| fetcher.supports_index_queries());
    if let Some(entries) = match index {
        Some(index) => {
            fetcher
                .get_by_index_prefix_with_values(
                    collection,
                    index,
                    &[NormalValue::String(parent_id.to_owned())],
                )
                .await?
        }
        None => None,
    } {
        let mut by_hash: RapidHashMap<Arc<str>, Vec<Arc<str>>> = RapidHashMap::default();
        for entry in entries {
            let Some(hash) = entry.values.get(1).and_then(NormalValue::as_str) else {
                continue;
            };
            by_hash
                .entry(Arc::from(hash))
                .or_default()
                .push(Arc::from(entry.doc_id));
        }
        return Ok(by_hash
            .into_iter()
            .map(|(hash, mut ids)| {
                ids.sort_unstable();
                ids.dedup();
                (hash, Arc::from(ids))
            })
            .collect());
    }

    let docs = fetcher
        .get_by_field_value(collection, CHUNK_SOURCE_ID_FIELD, parent_id)
        .await?;
    let mut by_hash: RapidHashMap<Arc<str>, Vec<Arc<str>>> = RapidHashMap::default();
    for copy in docs {
        let Some(fields) = ChunkFields::from_document(&copy).filter(|_| !copy.is_deleted()) else {
            continue;
        };
        let Some(hash) = fields.verified_hash() else {
            continue;
        };
        let Some(id) = copy.id() else {
            continue;
        };
        if fields.parent_id == parent_id {
            by_hash
                .entry(Arc::from(hash))
                .or_default()
                .push(Arc::from(id.to_string()));
        }
    }
    Ok(by_hash
        .into_iter()
        .map(|(hash, mut ids)| {
            ids.sort_unstable();
            ids.dedup();
            (hash, Arc::from(ids))
        })
        .collect())
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
