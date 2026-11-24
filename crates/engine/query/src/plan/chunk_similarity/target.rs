//! What a parent's `SIMILARITY` on a chunked embedding reads and scores.

use defra_core::vector::Metric;
use document::Document;
use schema::{ChunkedField, CollectionVersion, CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD};

use crate::error::{QueryError, Result};
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
    /// The chunk collection's `[_sourceID, hash]` index, which finds one listed chunk.
    pub source_index: Option<String>,
    /// The chunk collection's vector indexes on `field`, by ID and name.
    pub vector_indexes: Vec<(u32, String)>,
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
                let names: Vec<&str> = index.fields.iter().map(|f| f.name.as_str()).collect();
                names == [CHUNK_SOURCE_ID_FIELD, CHUNK_HASH_FIELD]
            })
            .map(|index| index.name.clone());
        let vector_indexes = chunks
            .indexes
            .iter()
            .filter(|index| {
                index.vector().is_some()
                    && index
                        .fields
                        .first()
                        .is_some_and(|field| field.name == similarity.target_field)
            })
            .map(|index| (index.id, index.name.clone()))
            .collect();
        Some(Self {
            parent: parent.name.clone(),
            chunked: chunked.clone(),
            chunks: chunks.name.clone(),
            source_index,
            vector_indexes,
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
        if vector.len() != self.vector.len() {
            return Err(QueryError::execution(format!(
                "source and vector must be of the same length. Source: {}, Vector: {}, Field: {}.{}",
                vector.len(),
                self.vector.len(),
                self.chunks,
                self.field
            )));
        }
        score_vectors(self.metric, &vector, &self.vector).map(Some)
    }

    /// The name of the chunk vector index with ID `id`.
    pub(crate) fn vector_index_name(&self, id: u32) -> Option<&str> {
        self.vector_indexes
            .iter()
            .find(|(index_id, _)| *index_id == id)
            .map(|(_, name)| name.as_str())
    }
}

/// The documents a chunked similarity read, which explain reports.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Reads {
    pub docs: u64,
    pub fields: u64,
    pub index_lookups: u64,
}

impl Reads {
    pub(crate) fn count(&mut self, doc: &Document) {
        self.docs += 1;
        self.fields += doc.len() as u64;
    }
}
