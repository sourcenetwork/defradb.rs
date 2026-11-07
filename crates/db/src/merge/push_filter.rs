//! A filtered peer gets a `@chunk` chunk only together with its parent, so
//! replay and retry decide a chunk by its parent's filter on the parent's
//! document, never by the chunk's own document.

use rapidhash::{HashMapExt, RapidHashMap};
use storage::corekv::{Reader, Store};

use crate::collection::Collection;
use crate::database::DB;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FilterDecision {
    Pass,
    Refuse,
    /// A chunk passes only with its parent, which must be sent with it.
    PassWithParent(ParentRef),
}

/// The parent a passing chunk is sent with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParentRef {
    pub(crate) doc_id: String,
    pub(crate) doc_short_id: u64,
}

pub(crate) enum DecidingFilter<'a> {
    Own(&'a p2p::ReplicationFilter),
    /// A `@chunk` collection is decided by its parent's filter on the parent's document.
    Parent {
        parent: Collection,
        filter: &'a p2p::ReplicationFilter,
    },
}

pub(crate) fn deciding_filter<'a, S>(
    db: &DB<S>,
    collection: &Collection,
    filters: &'a p2p::ReplicationFilters,
) -> Result<Option<DecidingFilter<'a>>, String>
where
    S: Store + 'static,
{
    if let Some(filter) = filters.get(collection.collection_id()) {
        return Ok(Some(DecidingFilter::Own(filter)));
    }
    let Some(source) = &collection.schema().chunk_of else {
        return Ok(None);
    };
    let parent = db
        .get_collection(&source.collection)
        .map_err(|e| format!("failed to get chunk parent collection: {e}"))?
        .ok_or_else(|| {
            format!(
                "chunk collection '{}' names parent collection '{}', which is not found",
                collection.name(),
                source.collection
            )
        })?;
    Ok(filters
        .get(parent.collection_id())
        .map(|filter| DecidingFilter::Parent { parent, filter }))
}

pub(crate) async fn stored_document<R>(
    datastore: &R,
    collection: &Collection,
    doc_short_id: u64,
) -> Result<Option<document::Document>, String>
where
    R: Reader + ?Sized,
{
    let doc_key = storage::keys::doc_key(collection.collection_id(), doc_short_id);
    let Some(doc_data) = datastore
        .get(&doc_key)
        .await
        .map_err(|e| format!("failed to read document for replication filter: {}", e))?
    else {
        return Ok(None);
    };
    crdt::stored::document_from_storage(&doc_data, collection.schema_arc(), collection.crdts())
        .map(Some)
        .map_err(|e| format!("failed to decode document for replication filter: {}", e))
}

pub(crate) async fn document_matches_filter<R>(
    datastore: &R,
    collection: &Collection,
    doc_short_id: u64,
    filter: &p2p::ReplicationFilter,
    matcher: &dyn p2p::replicator::ReplicationFilterMatcher,
) -> Result<bool, String>
where
    R: Reader + ?Sized,
{
    let Some(doc) = stored_document(datastore, collection, doc_short_id).await? else {
        return Ok(false);
    };
    let document_json = serde_json::Value::Object(
        doc.to_map()
            .map_err(|e| format!("failed to encode document for replication filter: {}", e))?
            .into_iter()
            .collect(),
    );
    Ok(matcher.matches("", filter, &document_json))
}

/// Filter checks for one peer, remembering each parent's result so a
/// parent with many chunks is read once.
pub(crate) struct FilterCheck<'a, R>
where
    R: Reader + ?Sized,
{
    pub(crate) datastore: &'a R,
    pub(crate) systemstore: &'a datastore::NamespaceView,
    pub(crate) matcher: &'a dyn p2p::replicator::ReplicationFilterMatcher,
    pub(crate) peer: &'a str,
    /// Each parent's result: its short ID when it passed, `None` when refused.
    parents: RapidHashMap<String, Option<u64>>,
    /// Chunks refused because their parent is not on this node.
    pub(crate) without_parent: usize,
}

impl<'a, R> FilterCheck<'a, R>
where
    R: Reader + ?Sized,
{
    pub(crate) fn new(
        datastore: &'a R,
        systemstore: &'a datastore::NamespaceView,
        matcher: &'a dyn p2p::replicator::ReplicationFilterMatcher,
        peer: &'a str,
    ) -> Self {
        Self {
            datastore,
            systemstore,
            matcher,
            peer,
            parents: RapidHashMap::new(),
            without_parent: 0,
        }
    }

    pub(crate) async fn decide(
        &mut self,
        collection: &Collection,
        doc_short_id: u64,
        deciding: &DecidingFilter<'_>,
    ) -> Result<FilterDecision, String> {
        let passed = match deciding {
            DecidingFilter::Own(filter) => {
                document_matches_filter(
                    self.datastore,
                    collection,
                    doc_short_id,
                    filter,
                    self.matcher,
                )
                .await?
            }
            DecidingFilter::Parent { parent, filter } => {
                return self
                    .decide_chunk(collection, doc_short_id, parent, filter)
                    .await;
            }
        };
        Ok(if passed {
            FilterDecision::Pass
        } else {
            FilterDecision::Refuse
        })
    }

    async fn decide_chunk(
        &mut self,
        chunks_col: &Collection,
        doc_short_id: u64,
        parent: &Collection,
        filter: &p2p::ReplicationFilter,
    ) -> Result<FilterDecision, String> {
        let peer = self.peer;
        let refuse = |reason: &str, warn: bool| {
            if warn {
                tracing::warn!(peer, chunks = %chunks_col.name(), doc_short_id, reason, "filtered replicator: chunk not sent");
            } else {
                tracing::debug!(peer, chunks = %chunks_col.name(), parent = %parent.name(), doc_short_id, reason, "filtered replicator: chunk not sent");
            }
            FilterDecision::Refuse
        };
        let Some(chunk) = stored_document(self.datastore, chunks_col, doc_short_id).await? else {
            return Ok(refuse("no chunk document", true));
        };
        let Some(document::NormalValue::String(parent_id)) =
            chunk.get(schema::CHUNK_SOURCE_ID_FIELD)
        else {
            return Ok(refuse("no _sourceID", true));
        };
        let passed = match self.parents.get(parent_id) {
            Some(passed) => *passed,
            None => {
                let Some(parent_ref) = crate::docid::map::get_doc_ref(self.systemstore, parent_id)
                    .await
                    .map_err(|e| {
                        format!("chunk {doc_short_id}: parent {parent_id} lookup failed: {e}")
                    })?
                else {
                    // Its parent was never sent, so the chunk is not sent either.
                    self.without_parent += 1;
                    return Ok(refuse("parent not on this node", false));
                };
                let passed = document_matches_filter(
                    self.datastore,
                    parent,
                    parent_ref.doc_short_id,
                    filter,
                    self.matcher,
                )
                .await?
                .then_some(parent_ref.doc_short_id);
                self.parents.insert(parent_id.clone(), passed);
                passed
            }
        };
        Ok(match passed {
            Some(doc_short_id) => FilterDecision::PassWithParent(ParentRef {
                doc_id: parent_id.clone(),
                doc_short_id,
            }),
            None => refuse("parent does not match", false),
        })
    }
}
