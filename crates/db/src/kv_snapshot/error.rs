use thiserror::Error;

/// Errors from exporting or importing a KV snapshot.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KvSnapshotError {
    #[error("collection name can't be empty")]
    CollectionNameEmpty,

    #[error("collection not found. Name: {0}")]
    CollectionNotFound(String),

    #[error("the KV snapshot ends inside a frame")]
    Truncated,

    #[error("a KV snapshot frame is larger than 4 GiB")]
    FrameTooLarge,

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
         IDs {doc_short_ids:?}, so their DocIDs cannot be recovered and they were not imported \
         ({imported} other documents were); export them with history"
    )]
    NoHistory {
        doc_short_ids: Vec<u64>,
        imported: usize,
    },

    #[error("document short ID {doc_short_id} of the KV snapshot did not merge: {reason}")]
    NotMerged { doc_short_id: u64, reason: String },
}
