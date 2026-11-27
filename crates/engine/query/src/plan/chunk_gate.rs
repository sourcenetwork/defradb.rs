//! Read checks for chunks. A chunk has no ACP entry, so its live parent decides.

use std::sync::Arc;

use document::Document;
use kovan::Atom;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::{RapidHashMap, RapidHashSet};
use schema::{ChunkedField, CollectionVersion};

use super::chunk_once::{all_copies, shown_copy, shown_copy_from_candidates, source_index};
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

/// A parent's DocID and a chunk hash.
type ChunkKey = (String, String);

/// The DocIDs of every copy of a parent, by chunk hash.
type CopiesByHash = RapidHashMap<Arc<str>, Arc<[Arc<str>]>>;

/// The chunk hashes seen for each parent in a listing.
type HashesSeenByParent = HopscotchMap<String, Arc<Atom<RapidHashSet<String>>>, RandomState>;

/// The read check for one chunk collection.
#[derive(Clone)]
pub(crate) struct ChunkGate {
    fetcher: Option<Arc<dyn DocFetcher>>,
    chunk_collection: Arc<str>,
    source_index: Option<String>,
    parent: Option<(String, ChunkedField)>,
    parent_acp: Option<AcpReadCheck>,
    parent_app: Option<AppReadCheck>,
    // Chunks of one parent share its decision, so it is made once per query.
    decided: Arc<HopscotchMap<String, ParentDecision, RandomState>>,
    // The copy of each chunk this query shows, picked once.
    shown: Arc<HopscotchMap<ChunkKey, Arc<str>, RandomState>>,
    // A listing reads many hashes from one parent. After the second, one
    // parent-wide index scan and one batched document fetch replace a point
    // query per hash.
    copies: Arc<HopscotchMap<String, Arc<CopiesByHash>, RandomState>>,
    seen: Arc<HashesSeenByParent>,
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
            source_index: source_index(chunks_col),
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
            shown: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            copies: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            seen: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
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
        if !decision.is_some_and(|hashes| hashes.contains(hash)) {
            return Ok(false);
        }
        if let Some(copies) = self.copies.get(&chunk.parent_id) {
            if let Some(candidates) = copies.get(hash) {
                return shown_copy_from_candidates(
                    fetcher.as_ref(),
                    &self.chunk_collection,
                    &chunk,
                    hash,
                    doc_id,
                    candidates,
                )
                .await;
            }
            return Ok(false);
        }
        if self.seen_hashes(&chunk.parent_id, hash) > 1 {
            let copies = all_copies(
                fetcher.as_ref(),
                &self.chunk_collection,
                self.source_index.as_deref(),
                &chunk.parent_id,
            )
            .await?;
            let copies = self
                .copies
                .get_or_insert(chunk.parent_id.clone(), Arc::new(copies));
            return match copies.get(hash) {
                Some(candidates) => {
                    shown_copy_from_candidates(
                        fetcher.as_ref(),
                        &self.chunk_collection,
                        &chunk,
                        hash,
                        doc_id,
                        candidates,
                    )
                    .await
                }
                None => Ok(false),
            };
        }
        let key = (chunk.parent_id.clone(), hash.to_owned());
        let shown = match self.shown.get(&key) {
            Some(shown) => shown,
            None => {
                let shown = shown_copy(
                    fetcher.as_ref(),
                    &self.chunk_collection,
                    self.source_index.as_deref(),
                    &chunk,
                    hash,
                    doc_id,
                )
                .await?;
                self.shown.get_or_insert(key, Arc::from(shown))
            }
        };
        Ok(&*shown == doc_id)
    }

    fn seen_hashes(&self, parent_id: &str, hash: &str) -> usize {
        let seen = self.seen.get_or_insert(
            parent_id.to_owned(),
            Arc::new(Atom::new(RapidHashSet::default())),
        );
        let mut count = 0;
        seen.rcu(|hashes| {
            let mut hashes = hashes.clone();
            hashes.insert(hash.to_owned());
            count = hashes.len();
            hashes
        });
        count
    }

    async fn decide(
        &self,
        fetcher: &dyn DocFetcher,
        parent: &str,
        chunked: &ChunkedField,
        parent_id: &str,
    ) -> Result<ParentDecision> {
        let Some(doc) = get_projected(fetcher, parent, parent_id, &parent_fields(chunked))
            .await?
            .filter(|doc| !doc.is_deleted())
        else {
            return Ok(None);
        };
        // The cut is the costly step, so a reader who may not see the parent never pays for it.
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
        Ok(parent_hashes(&doc, chunked).map(Arc::new))
    }
}
