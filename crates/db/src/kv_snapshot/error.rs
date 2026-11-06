use thiserror::Error;

/// Errors from importing a KV snapshot.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KvSnapshotError {
    #[error("collection not found. Name: {0}")]
    CollectionNotFound(String),

    #[error("the KV snapshot ends inside a frame")]
    Truncated,

    #[error("the KV snapshot stream failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("the field mapping is not valid JSON: {0}")]
    Mapping(#[from] serde_json::Error),

    #[error(
        "the field mapping names field {name} ({field_id}), which collection {collection} does \
         not have: add the field before importing"
    )]
    FieldMissing {
        collection: String,
        name: String,
        field_id: String,
    },

    #[error(transparent)]
    Database(#[from] crate::Error),

    #[error(transparent)]
    Document(#[from] document::Error),

    #[error(transparent)]
    Query(#[from] query::error::QueryError),

    #[error("a KV snapshot frame cannot be read: {0}")]
    Malformed(String),

    #[error(
        "the KV snapshot holds no heads for document short ID {doc_short_id}: a datastore-only \
         snapshot carries no DocIDs or history, so it cannot be imported; export it without \
         datastore-only"
    )]
    NoHeads { doc_short_id: u64 },

    #[error(
        "the KV snapshot holds no history back to the first version of the documents with short \
         IDs {doc_short_ids:?}: it has no create block for them, and a DocID is derived from the \
         create block, so their DocIDs cannot be recovered and they were not imported \
         ({imported} other documents were): Go Shinzo exports only a document's newest version, \
         so a document updated since it was created has to be replicated from a peer instead"
    )]
    NoHistory {
        doc_short_ids: Vec<u64>,
        imported: usize,
    },

    #[error(
        "the KV snapshot's documents {documents:?} are encrypted, and it carries neither their \
         encryption blocks nor their keys, so their values cannot be read and they were not \
         imported ({imported} other documents were, and the documents with short IDs \
         {without_history:?} were not, having no history back to their first version): Go \
         Shinzo exports neither, so an encrypted document has to be replicated from a peer that \
         holds its keys"
    )]
    Unreadable {
        documents: Vec<String>,
        without_history: Vec<u64>,
        imported: usize,
    },

    #[error("document short ID {doc_short_id} of the KV snapshot did not merge: {reason}")]
    NotMerged { doc_short_id: u64, reason: String },
}
