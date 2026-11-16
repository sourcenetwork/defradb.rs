//! Read checks for chunks. A chunk has no ACP entry, so its live parent decides.

use std::sync::Arc;

use document::Document;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashSet;
use schema::{ChunkedField, CollectionVersion};

use super::chunk_parent::{chunk_fields, parent_fields, parent_hashes, ChunkFields};
use super::permission_filter::AcpReadCheck;
use crate::access_hooks::AppReadCheck;
use crate::error::Result;
use crate::fetcher::DocFetcher;

/// Reads one document, validating only `fields` where the fetcher can.
pub(crate) async fn get_projected(
    fetcher: &dyn DocFetcher,
    collection: &str,
    doc_id: &str,
    fields: &RapidHashSet<String>,
) -> Result<Option<Document>> {
    let ids = [doc_id.to_string()];
    let found = match fetcher
        .get_by_ids_projected(collection, &ids, fields)
        .await?
    {
        Some(found) => found,
        None => fetcher.get_by_ids(collection, &ids).await?,
    };
    Ok(found.into_docs().into_iter().next())
}

/// The chunk hashes a parent's current text uses, or `None` when the reader
/// may not see the parent or it isn't live.
type ParentDecision = Option<Arc<RapidHashSet<String>>>;

/// The read check for one chunk collection.
#[derive(Clone)]
pub(crate) struct ChunkGate {
    fetcher: Option<Arc<dyn DocFetcher>>,
    chunk_collection: Arc<str>,
    parent: Option<(String, ChunkedField)>,
    parent_acp: Option<AcpReadCheck>,
    parent_app: Option<AppReadCheck>,
    // Chunks of one parent share its decision, so it is made once per query.
    decided: Arc<HopscotchMap<String, ParentDecision, RandomState>>,
}

impl ChunkGate {
    /// With no fetcher or parent to check against, every chunk is denied rather than shown unchecked.
    pub(crate) fn new(
        fetcher: Option<Arc<dyn DocFetcher>>,
        chunks_col: &CollectionVersion,
        parent: Option<&CollectionVersion>,
        parent_acp: Option<AcpReadCheck>,
        parent_app: Option<AppReadCheck>,
    ) -> Self {
        let gate = Self {
            fetcher,
            chunk_collection: Arc::from(chunks_col.name.as_str()),
            parent: chunks_col
                .chunk_of
                .as_ref()
                .zip(parent)
                .and_then(|(source, parent)| {
                    parent
                        .chunked_field(&source.field_name)
                        .map(|chunked| (parent.name.clone(), chunked.clone()))
                }),
            parent_acp,
            parent_app,
            decided: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
        };
        if gate.fetcher.is_none() || gate.parent.is_none() {
            tracing::warn!(
                chunks = %gate.chunk_collection,
                "chunk read gate has no fetcher or parent collection; every chunk is hidden"
            );
        }
        gate
    }

    /// Whether the chunk `doc_id` may show. `row` holds its fields when the
    /// scan carried them; otherwise they are read again.
    pub(crate) async fn allows(&self, doc_id: &str, row: Option<ChunkFields>) -> Result<bool> {
        let (Some(fetcher), Some((parent, chunked))) = (&self.fetcher, &self.parent) else {
            return Ok(false);
        };
        let chunk = match row {
            Some(chunk) => chunk,
            None => {
                let found = get_projected(
                    fetcher.as_ref(),
                    &self.chunk_collection,
                    doc_id,
                    &chunk_fields(),
                )
                .await?;
                let Some(chunk) = found.as_ref().and_then(ChunkFields::from_document) else {
                    return Ok(false);
                };
                chunk
            }
        };
        let Some(hash) = chunk.verified_hash() else {
            tracing::debug!(chunks = %self.chunk_collection, %doc_id, "chunk hash does not match its text; hidden");
            return Ok(false);
        };
        let decision = match self.decided.get(&chunk.parent_id) {
            Some(decision) => decision,
            None => {
                let decision = self
                    .decide(fetcher.as_ref(), parent, chunked, &chunk.parent_id)
                    .await?;
                self.decided
                    .get_or_insert(chunk.parent_id.clone(), decision)
            }
        };
        Ok(decision.is_some_and(|hashes| hashes.contains(hash)))
    }

    async fn decide(
        &self,
        fetcher: &dyn DocFetcher,
        parent: &str,
        chunked: &ChunkedField,
        parent_id: &str,
    ) -> Result<ParentDecision> {
        let Some(hashes) = get_projected(fetcher, parent, parent_id, &parent_fields(chunked))
            .await?
            .and_then(|doc| parent_hashes(&doc, chunked))
        else {
            return Ok(None);
        };
        if let Some(acp) = &self.parent_acp {
            if !acp.allows(parent_id).await? {
                return Ok(None);
            }
        }
        if let Some(app) = &self.parent_app {
            if !app.allows(parent_id).await? {
                return Ok(None);
            }
        }
        Ok(Some(Arc::new(hashes)))
    }
}
