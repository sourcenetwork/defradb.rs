//! Whether a filtered replicator takes a document. A `@chunk` chunk follows its parent.

use datastore::NamespaceView;
use document::NormalValue;
use p2p::replicator::ReplicationFilterMatcher;
use p2p::{Admission, ReplicatorInfo};
use rapidhash::{HashMapExt, RapidHashMap};
use schema::CHUNK_SOURCE_ID_FIELD;
use storage::corekv::{Reader, Store};

use crate::collection::Collection;
use crate::database::DB;

#[derive(Debug, thiserror::Error)]
pub(crate) enum FollowError {
    #[error("failed to load the parent collection of chunk collection '{chunks}': {source}")]
    ParentCollection {
        chunks: String,
        source: crate::error::Error,
    },
    #[error("failed to read document {doc_short_id} for replication filter: {source}")]
    Read {
        doc_short_id: u64,
        source: storage::Error,
    },
    #[error("failed to decode document {doc_short_id} for replication filter: {source}")]
    Decode {
        doc_short_id: u64,
        source: document::Error,
    },
    #[error("failed to look up parent {parent_id} of a chunk: {source}")]
    ParentLookup {
        parent_id: String,
        source: crate::error::Error,
    },
}

/// What a replicator takes of one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Decision {
    Take,
    Refuse,
    /// A chunk goes to a filtered peer only together with this parent.
    TakeWithParent(ParentRef),
}

/// The parent a passing chunk is sent with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParentRef {
    pub(super) doc_id: String,
    pub(super) doc_short_id: u64,
}

/// The collection whose documents decide one collection's documents.
pub(super) struct Deciding {
    collection_id: String,
    /// The parent collection, for a `@chunk` collection.
    pub(super) parent: Option<Collection>,
}

impl Deciding {
    pub(super) fn of<S>(db: &DB<S>, collection: &Collection) -> Result<Self, FollowError>
    where
        S: Store,
    {
        let parent =
            crate::chunk::access::parent_collection(db, collection.schema()).map_err(|source| {
                FollowError::ParentCollection {
                    chunks: collection.name().to_string(),
                    source,
                }
            })?;
        Ok(Self {
            collection_id: collection.collection_id().to_string(),
            parent,
        })
    }

    fn decided_in(&self) -> &str {
        self.parent
            .as_ref()
            .map_or(self.collection_id.as_str(), Collection::collection_id)
    }
}

/// [`ReplicatorInfo::admission`] for one replicator, reading each parent once for all its chunks.
pub(super) struct ReplicaFilter<'a, R>
where
    R: Reader + ?Sized,
{
    datastore: &'a R,
    systemstore: &'a NamespaceView,
    matcher: &'a dyn ReplicationFilterMatcher,
    replicator: &'a ReplicatorInfo,
    /// Each parent's result: its short ID when it passed, `None` when refused.
    parents: RapidHashMap<String, Option<u64>>,
    pub(super) without_parent: usize,
}

impl<'a, R> ReplicaFilter<'a, R>
where
    R: Reader + ?Sized,
{
    pub(super) fn new(
        datastore: &'a R,
        systemstore: &'a NamespaceView,
        matcher: &'a dyn ReplicationFilterMatcher,
        replicator: &'a ReplicatorInfo,
    ) -> Self {
        Self {
            datastore,
            systemstore,
            matcher,
            replicator,
            parents: RapidHashMap::new(),
            without_parent: 0,
        }
    }

    pub(super) async fn decide(
        &mut self,
        collection: &Collection,
        deciding: &Deciding,
        doc_short_id: u64,
    ) -> Result<Decision, FollowError> {
        let replicator = self.replicator;
        let matcher = self.matcher;
        let admission = |document: Option<&serde_json::Value>| {
            replicator.admission(
                matcher,
                collection.collection_id(),
                deciding.decided_in(),
                document,
            )
        };
        if admission(None) == Admission::Admit {
            return Ok(Decision::Take);
        }
        let Some(parent) = &deciding.parent else {
            let document = document_json(self.datastore, collection, doc_short_id).await?;
            return Ok(match admission(document.as_ref()) {
                Admission::Admit => Decision::Take,
                Admission::NoDocument | Admission::FilterMiss => Decision::Refuse,
            });
        };
        let Some(parent_id) = parent_id_of(self.datastore, collection, doc_short_id).await? else {
            tracing::warn!(
                peer = %replicator.peer_id_str(),
                chunks = %collection.name(),
                doc_short_id,
                "filtered replicator: chunk without a parent not sent"
            );
            return Ok(Decision::Refuse);
        };
        let passed = match self.parents.get(&parent_id) {
            Some(passed) => *passed,
            None => {
                let passed = self.parent_passes(collection, parent, &parent_id).await?;
                self.parents.insert(parent_id.clone(), passed);
                passed
            }
        };
        Ok(match passed {
            Some(doc_short_id) => Decision::TakeWithParent(ParentRef {
                doc_id: parent_id,
                doc_short_id,
            }),
            None => Decision::Refuse,
        })
    }

    async fn parent_passes(
        &mut self,
        chunks_col: &Collection,
        parent: &Collection,
        parent_id: &str,
    ) -> Result<Option<u64>, FollowError> {
        let Some(parent_ref) = crate::docid::map::get_doc_ref(self.systemstore, parent_id)
            .await
            .map_err(|source| FollowError::ParentLookup {
                parent_id: parent_id.to_string(),
                source,
            })?
        else {
            // A parent that never reached this node was never sent, so its chunks are not either.
            self.without_parent += 1;
            return Ok(None);
        };
        let document = document_json(self.datastore, parent, parent_ref.doc_short_id).await?;
        let admission = self.replicator.admission(
            self.matcher,
            chunks_col.collection_id(),
            parent.collection_id(),
            document.as_ref(),
        );
        Ok((admission == Admission::Admit).then_some(parent_ref.doc_short_id))
    }
}

/// The stored document as the JSON a replication filter matches, `None` when it is missing.
pub(super) async fn document_json<R>(
    datastore: &R,
    collection: &Collection,
    doc_short_id: u64,
) -> Result<Option<serde_json::Value>, FollowError>
where
    R: Reader + ?Sized,
{
    let Some(doc) = stored_document(datastore, collection, doc_short_id).await? else {
        return Ok(None);
    };
    let map = doc.to_map().map_err(|source| FollowError::Decode {
        doc_short_id,
        source,
    })?;
    Ok(Some(serde_json::Value::Object(map.into_iter().collect())))
}

async fn stored_document<R>(
    datastore: &R,
    collection: &Collection,
    doc_short_id: u64,
) -> Result<Option<document::Document>, FollowError>
where
    R: Reader + ?Sized,
{
    let key = storage::keys::doc_key(collection.collection_id(), doc_short_id);
    let Some(bytes) = datastore
        .get(&key)
        .await
        .map_err(|source| FollowError::Read {
            doc_short_id,
            source,
        })?
    else {
        return Ok(None);
    };
    crdt::stored::document_from_storage(&bytes, collection.schema_arc(), collection.crdts())
        .map(Some)
        .map_err(|source| FollowError::Decode {
            doc_short_id,
            source,
        })
}

// The stored chunk's parent, read even when the chunk is deleted.
async fn parent_id_of<R>(
    datastore: &R,
    chunks_col: &Collection,
    chunk_short_id: u64,
) -> Result<Option<String>, FollowError>
where
    R: Reader + ?Sized,
{
    let Some(chunk) = stored_document(datastore, chunks_col, chunk_short_id).await? else {
        return Ok(None);
    };
    Ok(match chunk.get(CHUNK_SOURCE_ID_FIELD) {
        Some(NormalValue::String(parent_id)) => Some(parent_id.clone()),
        _ => None,
    })
}
