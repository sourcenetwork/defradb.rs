//! Read checks for chunk collections.
//!
//! A chunk has no policy and is never registered with ACP, so no grant can make
//! it more visible than its parent. A chunk is readable only while its parent is
//! live, the parent's current list names it, and the reader may read the parent.

use std::sync::Arc;

use document::Document;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashSet;
use schema::{ChunkedField, CollectionVersion};

use super::chunk_parent::{
    chunk_fields, chunk_parent_id, parent_fields, parent_hashes, parent_lists, verified_hash,
};
use super::permission_filter::AcpReadCheck;
use crate::access_hooks::AppReadCheck;
use crate::error::Result;
use crate::fetcher::DocFetcher;

/// Reads one document, validating only `fields` where the fetcher can.
async fn get_projected(
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

/// The parent a chunk's visibility follows, if that parent is live and its
/// current list names the chunk.
pub(crate) async fn live_parent_of(
    fetcher: &dyn DocFetcher,
    chunk_collection: &str,
    parent_collection: &str,
    chunked: &ChunkedField,
    chunk_doc_id: &str,
) -> Result<Option<String>> {
    let Some(chunk) =
        get_projected(fetcher, chunk_collection, chunk_doc_id, &chunk_fields()).await?
    else {
        return Ok(None);
    };
    let Some(parent_id) = chunk_parent_id(&chunk) else {
        return Ok(None);
    };
    let parent = get_projected(
        fetcher,
        parent_collection,
        parent_id,
        &parent_fields(chunked),
    )
    .await?;
    Ok(parent
        .filter(|parent| parent_lists(parent, &chunk, chunked))
        .map(|_| parent_id.to_string()))
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
        Self {
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
        }
    }

    pub(crate) async fn allows(&self, doc_id: &str) -> Result<bool> {
        let (Some(fetcher), Some((parent, chunked))) = (&self.fetcher, &self.parent) else {
            return Ok(false);
        };
        let Some(chunk) = get_projected(
            fetcher.as_ref(),
            &self.chunk_collection,
            doc_id,
            &chunk_fields(),
        )
        .await?
        else {
            return Ok(false);
        };
        let (Some(parent_id), Some(hash)) = (chunk_parent_id(&chunk), verified_hash(&chunk)) else {
            return Ok(false);
        };
        let decision = match self.decided.get(parent_id) {
            Some(decision) => decision,
            None => {
                let decision = self
                    .decide(fetcher.as_ref(), parent, chunked, parent_id)
                    .await?;
                self.decided.get_or_insert(parent_id.to_string(), decision)
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
