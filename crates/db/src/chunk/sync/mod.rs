//! Keeps a parent's chunk documents in step with its text, inside the parent's
//! own transaction.
//!
//! [`set_lists`] puts the new list on the parent before it is written, so the
//! text and its list are saved together. [`ChunkWrites`] then creates the
//! missing chunks, or deletes them all when the parent is deleted.
//! `proofs/tla/ParentChunks.tla` checks that this keeps them consistent.

mod lookup;
mod write;

use std::sync::Arc;

use defra_core::encryption::EncryptionConfig;
use defra_core::RequestContext;
use document::{Document, NormalValue};
use query::error::{QueryError, Result};
use query::mutator::DerivedWrite;
use query::prepared::PreparedInputMemo;
use rapidhash::RapidHashSet;
use schema::CollectionVersion;
use storage::corekv::Store;

use crate::chunk::list::{chunk_list, distinct};
use crate::chunk::split::{split, ChunkConfig};
use crate::collection::Collection;
use crate::database::DB;
use crate::event::emission::TxnBroadcaster;
use crate::index::IndexManager;
use crate::txn::DbTxn;

// As JSON, the form filtered replication matches against.
fn json(doc: &Document) -> Option<serde_json::Value> {
    Some(serde_json::Value::Object(
        doc.to_map().ok()?.into_iter().collect(),
    ))
}

/// The chunk documents one field's new text needs.
pub(crate) struct FieldChunks {
    field: String,
    chunk_collection: String,
    chunks: Vec<(String, String)>,
    encrypt: bool,
}

/// Splits every chunked field this write sets and puts its list on `doc`.
/// A null text gets a null list.
pub(crate) fn set_lists(
    schema: &CollectionVersion,
    doc: &mut Document,
    mut modified: Option<&mut RapidHashSet<String>>,
    encryption: Option<&EncryptionConfig>,
) -> Result<Vec<FieldChunks>> {
    let mut plans = Vec::new();
    for chunked in &schema.chunked_fields {
        let field = chunked.field_name.as_str();
        let written = match modified.as_deref() {
            Some(modified) => modified.contains(field),
            None => doc.get(field).is_some(),
        };
        if !written {
            continue;
        }
        let list_field = chunked.list_field();
        let text = doc
            .get(field)
            .and_then(NormalValue::as_str)
            .map(str::to_owned);
        match text {
            Some(text) => {
                let config = ChunkConfig {
                    max_chars: chunked.max_chars as usize,
                    overlap: chunked.overlap as usize,
                };
                let chunks = split(&text, &config);
                doc.set(
                    list_field.clone(),
                    NormalValue::StringArray(chunk_list(&text, &chunks)),
                )
                .map_err(list_error)?;
                plans.push(FieldChunks {
                    field: field.to_string(),
                    chunk_collection: chunked.collection(&schema.name),
                    chunks: distinct(&chunks)
                        .into_iter()
                        .map(|chunk| (chunk.text.clone(), chunk.hash.clone()))
                        .collect(),
                    encrypt: encryption.is_some_and(|config| config.should_encrypt_field(field)),
                });
            }
            None => doc
                .set(list_field.clone(), NormalValue::Null)
                .map_err(list_error)?,
        }
        if let Some(modified) = modified.as_deref_mut() {
            modified.insert(list_field);
        }
    }
    Ok(plans)
}

fn list_error(error: document::Error) -> QueryError {
    QueryError::execution(format!("chunk list: {error}"))
}

/// What a chunk write needs from the parent's write.
pub(crate) struct ChunkWrites<'a, S>
where
    S: Store + 'static,
{
    pub(crate) db: &'a Arc<DB<S>>,
    pub(crate) txn: &'a DbTxn<S>,
    pub(crate) context: &'a RequestContext,
    pub(crate) prepared_inputs: Option<&'a PreparedInputMemo>,
    pub(crate) broadcaster: Option<&'a Arc<dyn TxnBroadcaster>>,
}

struct ChunkCollection {
    collection: Collection,
    datastore: datastore::NamespaceView,
    systemstore: datastore::NamespaceView,
    index_manager: IndexManager,
    index_name: String,
}

impl<S> ChunkWrites<'_, S>
where
    S: Store + 'static,
{
    /// Creates the chunk documents in `plans` that don't exist yet. An existing
    /// chunk is never rewritten, since the same hash means the same content.
    pub(crate) async fn create_missing(
        &self,
        parent_doc_id: &str,
        plans: Vec<FieldChunks>,
    ) -> Result<Vec<DerivedWrite>> {
        let mut written = Vec::new();
        for plan in plans {
            let target = self.open(&plan.chunk_collection).await?;
            // An update keeps the text encrypted when earlier writes did, even
            // without the flag, so its chunks must be too.
            let encrypt = plan.encrypt || self.parent_encrypts(parent_doc_id, &plan.field).await?;
            for (text, hash) in plan.chunks {
                if self.exists(&target, parent_doc_id, &hash).await? {
                    continue;
                }
                written.push(
                    self.create(&target, parent_doc_id, text, hash, encrypt)
                        .await?,
                );
            }
        }
        Ok(written)
    }

    /// Deletes all chunk documents of `parent_doc_id`, old ones included.
    pub(crate) async fn delete_all(
        &self,
        parent: &CollectionVersion,
        parent_doc_id: &str,
    ) -> Result<Vec<DerivedWrite>> {
        let mut written = Vec::new();
        for chunked in &parent.chunked_fields {
            let target = self.open(&chunked.collection(&parent.name)).await?;
            for doc_short_id in self.chunks_of(&target, parent_doc_id).await? {
                if let Some(write) = self.delete(&target, doc_short_id).await? {
                    written.push(write);
                }
            }
        }
        Ok(written)
    }
}
