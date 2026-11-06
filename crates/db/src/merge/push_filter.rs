//! What a filtered replicator's replay and retry send: a document by its own
//! filter, a `@chunk` chunk by its parent's filter on the parent's document.

use rapidhash::{HashMapExt, RapidHashMap};
use storage::corekv::{Reader, Store};

use crate::collection::Collection;
use crate::database::DB;

/// The outcome of a filter check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilterDecision {
    Pass,
    Refuse,
    /// The chunk's parent is not here yet, so nothing can be decided.
    ParentMissing,
}

pub(crate) enum DecidingFilter<'a> {
    Own(&'a p2p::ReplicationFilter),
    /// A `@chunk` collection is decided by its parent's filter on the parent's document.
    Parent {
        parent: Collection,
        filter: &'a p2p::ReplicationFilter,
    },
}

/// The filter that decides `collection`'s documents, if the peer has one.
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
        .ok_or_else(|| format!("chunk parent collection '{}' not found", source.collection))?;
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
    parents: RapidHashMap<String, bool>,
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
        let refuse = |reason: &str| {
            tracing::debug!(
                peer,
                chunks = %chunks_col.name(),
                parent = %parent.name(),
                doc_short_id,
                reason,
                "filtered replicator: chunk not sent"
            );
            FilterDecision::Refuse
        };
        let Some(chunk) = stored_document(self.datastore, chunks_col, doc_short_id).await? else {
            return Ok(refuse("no chunk document"));
        };
        let Some(document::NormalValue::String(parent_id)) =
            chunk.get(schema::CHUNK_SOURCE_ID_FIELD)
        else {
            return Ok(refuse("no _sourceID"));
        };
        if let Some(&passed) = self.parents.get(parent_id) {
            return Ok(if passed {
                FilterDecision::Pass
            } else {
                refuse("parent does not match")
            });
        }
        let Some(parent_ref) = crate::docid::map::get_doc_ref(self.systemstore, parent_id)
            .await
            .map_err(|e| format!("chunk parent lookup failed: {e}"))?
        else {
            tracing::debug!(
                peer = self.peer,
                chunks = %chunks_col.name(),
                parent = %parent.name(),
                parent_id = %parent_id,
                "filtered replicator: chunk waits for its parent"
            );
            return Ok(FilterDecision::ParentMissing);
        };
        let passed = document_matches_filter(
            self.datastore,
            parent,
            parent_ref.doc_short_id,
            filter,
            self.matcher,
        )
        .await?;
        self.parents.insert(parent_id.clone(), passed);
        Ok(if passed {
            FilterDecision::Pass
        } else {
            refuse("parent does not match")
        })
    }
}
