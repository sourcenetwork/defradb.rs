//! Keeps a parent's chunk documents in step with its text, in the parent's transaction.

mod lookup;
mod prefetch;
mod remove;
mod write;

pub(crate) use lookup::Stored;
pub(crate) use prefetch::embedding_requests;
pub(crate) use remove::remove;
pub(crate) use write::chunk_document;

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

use crate::chunk::hash::distinct;
use crate::chunk::split::split;
use crate::collection::Collection;
use crate::database::DB;
use crate::event::emission::{FollowedParent, TxnBroadcaster};
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

/// Splits every chunked field this write sets to a text.
pub(crate) fn plan_chunks(
    schema: &CollectionVersion,
    doc: &Document,
    modified: Option<&RapidHashSet<String>>,
    encryption: Option<&EncryptionConfig>,
) -> Result<Vec<FieldChunks>> {
    let mut plans = Vec::new();
    for chunked in &schema.chunked_fields {
        let field = chunked.field_name.as_str();
        let written = match modified {
            Some(modified) => modified.contains(field),
            None => doc.get(field).is_some(),
        };
        if !written {
            continue;
        }
        // A text set to null uses no chunk, so its plan deletes them all.
        let chunks = match doc.get(field).and_then(NormalValue::as_str) {
            Some(text) => {
                let config = crate::chunk::chunk_config(&schema.name, chunked)
                    .map_err(|error| QueryError::execution(error.to_string()))?;
                distinct(&split(text, &config))
                    .into_iter()
                    .map(|chunk| (chunk.text.clone(), chunk.hash.clone()))
                    .collect()
            }
            None => Vec::new(),
        };
        plans.push(FieldChunks {
            field: field.to_string(),
            chunk_collection: chunked.collection(&schema.name),
            chunks,
            encrypt: encryption.is_some_and(|config| config.should_encrypt_field(field)),
        });
    }
    Ok(plans)
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
    pub(crate) follows: Option<FollowedParent>,
}

/// A chunk collection opened for writes in one transaction.
pub(crate) struct ChunkCollection {
    pub(crate) collection: Collection,
    pub(crate) datastore: datastore::NamespaceView,
    pub(crate) systemstore: datastore::NamespaceView,
    pub(crate) index_manager: IndexManager,
    index_name: String,
}

impl<S> ChunkWrites<'_, S>
where
    S: Store + 'static,
{
    /// A filtered replicator decides each chunk by `parent`. A chunk has none of its fields.
    pub(crate) fn following(mut self, parent: FollowedParent) -> Self {
        self.follows = Some(parent);
        self
    }

    /// Makes the chunk documents match `plans`; each new one names its field's link in `commit`.
    pub(crate) async fn sync_chunks(
        &self,
        parent_doc_id: &str,
        commit: &[u8],
        plans: Vec<FieldChunks>,
    ) -> Result<Vec<DerivedWrite>> {
        let links = defra_core::block::Block::from_dag_cbor(commit)
            .map_err(|e| QueryError::execution(format!("parent block: {e}")))?
            .links
            .unwrap_or_default();
        let mut missing = Vec::new();
        let mut first_miss = None;
        for plan in plans {
            let source = links
                .iter()
                .find(|link| link.name == plan.field)
                .map(|link| link.link);
            let target = self.open(&plan.chunk_collection).await?;
            let planned: RapidHashSet<&str> =
                plan.chunks.iter().map(|(_, hash)| hash.as_str()).collect();
            // TODO(https://github.com/sourcenetwork/defradb.rs/issues/3381): reuse trusts the vector of a chunk any peer sent.
            let stored = target.stored_of(parent_doc_id).await?;
            let settled = target
                .settle(parent_doc_id, &planned, &stored, source.as_ref())
                .await?;
            let mut docs = Vec::new();
            for (text, hash) in plan.chunks {
                if settled.held.contains(&hash) {
                    continue;
                }
                let mut doc = Self::chunk_document(&target, parent_doc_id, text, hash.clone())?;
                let named = match self.imported_source(&target, parent_doc_id, &hash)? {
                    // The commit is the backup's, so the chunk must not wait for it to merge here.
                    Some(named) => {
                        crate::docid::map::set_block_doc_id_mapping(
                            &target.systemstore,
                            &named.to_string(),
                            parent_doc_id,
                        )
                        .await
                        .map_err(|e| QueryError::execution(e.to_string()))?;
                        named
                    }
                    None => source.ok_or_else(|| {
                        QueryError::execution(format!("parent block has no {} link", plan.field))
                    })?,
                };
                crate::chunk::source::set_source(&mut doc, &named)
                    .map_err(|e| QueryError::execution(e.to_string()))?;
                if let Some(miss) = self.embed(&target, &mut doc)? {
                    first_miss.get_or_insert(miss);
                }
                docs.push((doc, hash));
            }
            // An update keeps the text encrypted when earlier writes did, even
            // without the flag, so its chunks must be too.
            let encrypt = !docs.is_empty()
                && (plan.encrypt || self.parent_encrypts(parent_doc_id, &plan.field).await?);
            missing.push((target, settled.delete, encrypt, docs));
        }
        // TODO(https://github.com/sourcenetwork/defradb.rs/issues/3380): one deadline bounds the whole wave.
        // On a miss nothing is written; every miss is noted for one wave.
        if let Some(miss) = first_miss {
            return Err(QueryError::NeedsInput(miss));
        }
        let mut written = Vec::new();
        for (target, unneeded, encrypt, docs) in missing {
            for doc_short_id in unneeded {
                written.extend(self.delete(&target, doc_short_id).await?);
            }
            for (doc, hash) in docs {
                written.push(
                    self.create(&target, parent_doc_id, doc, hash, encrypt)
                        .await?,
                );
            }
        }
        Ok(written)
    }

    /// Deletes all chunk documents of `parent_doc_id`.
    pub(crate) async fn delete_all(
        &self,
        parent: &CollectionVersion,
        parent_doc_id: &str,
    ) -> Result<Vec<DerivedWrite>> {
        let mut written = Vec::new();
        for chunked in &parent.chunked_fields {
            let target = self.open(&chunked.collection(&parent.name)).await?;
            for doc_short_id in target.chunks_of(parent_doc_id).await? {
                if let Some(write) = self.delete(&target, doc_short_id).await? {
                    written.push(write);
                }
            }
        }
        Ok(written)
    }
}
