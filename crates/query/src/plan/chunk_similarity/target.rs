//! What a parent's `SIMILARITY` on a chunked embedding reads and scores.

use defra_core::vector::Metric;
use document::Document;
use schema::{ChunkedField, CollectionVersion, CHUNK_SOURCE_ID_FIELD};

use crate::error::Result;
use crate::json_convert::normal_value_to_json;
use crate::mapper::Similarity;
use crate::plan::similarity::{score_vectors, SimilarityNode};

/// A `SIMILARITY` on a parent whose target embedding lives on its chunks.
#[derive(Debug, Clone)]
pub(crate) struct ChunkTarget {
    /// The parent collection.
    pub parent: String,
    /// The parent's `@chunk` field the chunks are cut from.
    pub chunked: ChunkedField,
    /// The chunk collection.
    pub chunks: String,
    /// The chunk collection's `_sourceID` index, which finds one parent's chunks.
    pub source_index: Option<String>,
    /// The embedding field on the chunks.
    pub field: String,
    /// The query vector.
    pub vector: Vec<f64>,
    /// The metric the chunk field's index ranks by.
    pub metric: Metric,
}

impl ChunkTarget {
    /// The target for `similarity` on `parent`, when `chunks` holds its embedding.
    pub(crate) fn new(
        parent: &CollectionVersion,
        chunks: &CollectionVersion,
        similarity: &Similarity,
    ) -> Option<Self> {
        let chunked = parent.chunked_field(&chunks.chunk_of.as_ref()?.field_name)?;
        let source_index = chunks
            .indexes
            .iter()
            .find(|index| {
                index
                    .fields
                    .first()
                    .is_some_and(|field| field.name == CHUNK_SOURCE_ID_FIELD)
            })
            .map(|index| index.name.clone());
        Some(Self {
            parent: parent.name.clone(),
            chunked: chunked.clone(),
            chunks: chunks.name.clone(),
            source_index,
            field: similarity.target_field.clone(),
            vector: similarity.vector.clone(),
            metric: crate::planner::vector_routing::scoring_metric(
                &chunks.indexes,
                &similarity.target_field,
                similarity.metric,
            )
            .engine_metric(),
        })
    }

    /// A chunk's score, or `None` when it has no vector.
    pub(crate) fn score(&self, chunk: &Document) -> Result<Option<f64>> {
        let Some(value) = chunk.get(&self.field) else {
            return Ok(None);
        };
        // Through JSON, as `SimilarityNode` reads it, so both paths score alike.
        let Some(vector) = SimilarityNode::extract_vector(&normal_value_to_json(value)?) else {
            return Ok(None);
        };
        score_vectors(self.metric, &vector, &self.vector).map(Some)
    }
}
