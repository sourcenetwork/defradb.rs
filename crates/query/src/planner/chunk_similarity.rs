//! Planning a `SIMILARITY` whose embedding lives on the collection's `@chunk` collection.

use std::sync::Arc;

use schema::{CollectionVersion, IndexDescription};

use super::builder::Planner;
use crate::document::DocumentMapping;
use crate::error::{QueryError, Result};
use crate::mapper::Filter;
use crate::mapper::{Requestable, Select, Similarity};
use crate::plan::chunk_similarity::{ChunkTarget, ListedInRow};
use crate::plan::{ChunkSimilarityNode, ChunkSimilarityScanNode};
use crate::planner::vector_routing::VectorRoute;
use crate::planner::PlanNode;

impl Planner {
    /// The chunk collections `collection`'s `@chunk` fields generate.
    fn chunk_collections(&self, collection: &CollectionVersion) -> Vec<Arc<CollectionVersion>> {
        collection
            .chunk_collection_names()
            .filter_map(|name| self.get_collection(&name))
            .collect()
    }

    /// What `similarity` reads, when its embedding lives on `collection`'s chunks.
    pub(super) fn chunk_target(
        &self,
        collection: &CollectionVersion,
        similarity: &Similarity,
    ) -> Option<ChunkTarget> {
        let candidates = self.chunk_collections(collection);
        let chunks = collection
            .chunk_embedding(&similarity.target_field, candidates.iter().map(Arc::as_ref))?;
        ChunkTarget::new(collection, chunks, similarity)
    }

    /// The indexes `select`'s similarities route through: `collection`'s own,
    /// and its chunks' vector indexes on embeddings it does not hold.
    pub(super) fn similarity_route_indexes(
        &self,
        select: &Select,
        collection: &CollectionVersion,
    ) -> Vec<IndexDescription> {
        let mut indexes = collection.indexes.clone();
        let candidates = self.chunk_collections(collection);
        for field in &select.fields {
            let Requestable::Similarity(similarity) = field else {
                continue;
            };
            let Some(chunks) = collection
                .chunk_embedding(&similarity.target_field, candidates.iter().map(Arc::as_ref))
            else {
                continue;
            };
            indexes.extend(
                chunks
                    .indexes
                    .iter()
                    .filter(|index| {
                        index.vector().is_some()
                            && index
                                .fields
                                .first()
                                .is_some_and(|field| field.name == similarity.target_field)
                    })
                    .cloned(),
            );
        }
        indexes
    }

    /// The parent scan for `route` when its similarity's embedding lives on the chunks.
    pub(super) fn chunk_route_scan(
        &self,
        route: &VectorRoute,
        select: &Select,
        collection: &CollectionVersion,
        mapping: &DocumentMapping,
        filter: Option<&Filter>,
    ) -> Result<Option<Box<dyn PlanNode>>> {
        let Some(similarity) = select.fields.iter().find_map(|field| match field {
            Requestable::Similarity(similarity) => Some(similarity),
            _ => None,
        }) else {
            return Ok(None);
        };
        let Some(target) = self.chunk_target(collection, similarity) else {
            return Ok(None);
        };
        let fetcher = self
            .fetcher
            .clone()
            .ok_or_else(|| QueryError::internal("a routed similarity query has no fetcher"))?;
        let mut scan = ChunkSimilarityScanNode::new(
            collection.clone(),
            mapping.clone(),
            target,
            route.index_id,
            route.k,
            similarity_slot(mapping, similarity)?,
            fetcher,
        );
        if let Some(doc_ids) = &select.doc_ids {
            scan = scan.with_doc_ids(doc_ids.clone());
        }
        if let Some(filter) = filter {
            scan = scan.with_filter(filter.clone());
        }
        Ok(Some(Box::new(scan)))
    }

    /// Adds a [`ChunkSimilarityNode`] for each similarity whose embedding lives
    /// on `collection`'s chunks, scoring each row by its best listed chunk.
    pub(super) fn add_chunk_similarity_nodes(
        &self,
        mut plan: Box<dyn PlanNode>,
        select: &Select,
        mapping: &DocumentMapping,
        collection: &CollectionVersion,
    ) -> Result<Box<dyn PlanNode>> {
        for field in &select.fields {
            let Requestable::Similarity(similarity) = field else {
                continue;
            };
            let Some(target) = self.chunk_target(collection, similarity) else {
                continue;
            };
            let similarity_index = similarity_slot(mapping, similarity)?;
            let in_row = mapping
                .first_index_of_name(&target.chunked.field_name)
                .zip(mapping.first_index_of_name(&target.chunked.list_field()))
                .map(|(text, list)| ListedInRow { text, list });
            plan = Box::new(ChunkSimilarityNode::new(
                plan,
                mapping.clone(),
                similarity_index,
                target,
                self.fetcher.clone(),
                in_row,
            ));
        }
        Ok(plan)
    }
}

/// Where `similarity`'s score goes in a row.
pub(super) fn similarity_slot(mapping: &DocumentMapping, similarity: &Similarity) -> Result<usize> {
    mapping
        .try_find_index_from_render_key(similarity.output_name())
        .ok_or_else(|| {
            QueryError::internal(format!(
                "similarity output '{}' not found in mapping render keys",
                similarity.output_name()
            ))
        })
}

#[cfg(test)]
#[path = "chunk_similarity_tests.rs"]
mod tests;
